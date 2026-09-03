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
//! **And the fourth, which is the one people read this command for: the BINDING.** The three
//! above are two independent legs — the first over ids the caller supplied, the other two over
//! bytes the caller supplied — and nothing in either takes the other's input
//! (`rendered_output_hash_v1` is a keyed hash of the *ids* on every shipped family and never
//! reaches a byte of text). So an executor holding one honest claim can derive an artifact from a
//! completely different answer, file it against that claim (same `claim_id`, same `output_root`,
//! the two fields the chain checks) and hand a verifier the answer the artifact really came from
//! beside the ids the claim really committed; every check above passes. The join is the RENDERING
//! (ADR-0077 Decision 2: the answer's bytes are every token's bytes concatenated), so with the
//! ids, the whole job context and the tokenizer that context pins, this command renders the
//! claim's ids and re-runs the derivation over THOSE bytes
//! (`misaka_palw_derive::verify_bound`) and reports `binding_checked: true`. Without all four it
//! reports `binding_checked: false` and its verdict word is
//! `consistent-given-the-supplied-answer`, never a bare `consistent`.
//!
//! **What the chain does not have, and what the verifier must therefore hold.** The answer's
//! `output_token_ids` are on no chain in any form — the claim commits `output_root` and nothing
//! else, because ADR-0044 Decision 8's sentence about not silently publishing prompts applies to
//! answers word for word. So the gateway's own response is the second half of the evidence: it
//! carries `output_token_ids`, `job_context` (and its hash), `family` and the canonical DSL beside
//! the answer, and this command takes that file. Verification is a comparison of two independent
//! sources, and a verifier who had both from the same source would be checking nothing.
//!
//! Nothing here signs, spends or submits: two reads and some arithmetic.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_VERSION, PalwDerivedArtifactV1, derived_id_v1, kind};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwDerivedArtifactsResponse, RpcPalwDerivedArtifact};
use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use misaka_palw_base0::e2e_drill::PalwRcFamilyV1;
use misaka_palw_base0::tokenizer::QwenTokenizer;
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
         misaka palw derived-verify {} --answer <response.json> --tokenizer <tokenizer.json>",
        r.claim_id
    );
    println!(
        "    (without --tokenizer the verdict is `consistent-given-the-supplied-answer`: the derivation re-runs over \
         bytes YOU supplied and nothing says it came from this claim's inference)"
    );
}

fn short(hex: &str) -> &str {
    &hex[..hex.len().min(16)]
}

// ---------------------------------------------------------------------------------------------
// `misaka palw derived-verify` — the consumer's arithmetic
// ---------------------------------------------------------------------------------------------

/// What the consumer holds beside the chain: the answer's ids, the job's CONTEXT (and hence its
/// hash), the family whose rendered-hash rule applies, and the answer bytes the derivation
/// consumed.
///
/// **The context and not only its hash.** `output_root` needs the hash; the binding needs to know
/// which tokenizer may render those ids, and that is `tokenizer_id`, a FIELD of
/// [`PalwJobContextV2`]. When the context is here the hash is DERIVED from it — see
/// [`read_material`] — so the tokenizer a verifier renders under and the root it checks can never
/// come from two different contexts.
struct ConsumerMaterial {
    output_token_ids: Option<Vec<u32>>,
    job_context: Option<PalwJobContextV2>,
    job_context_hash: Option<Hash64>,
    family: Option<PalwRcFamilyV1>,
    answer: Option<Vec<u8>>,
}

/// Read the gateway's own response — the `misaka` block of a chat completion, the block itself, or
/// a bare JSON array of ids — plus any flag overrides.
///
/// A bare array is accepted because a consumer may keep only the ids; then the other inputs come
/// from flags. A missing one narrows what can be checked and the verdict says so by name — and
/// with none of them AND no DSL, `verify` refuses rather than printing a pass it did not earn.
///
/// **A supplied hash never wins over a supplied context.** When both arrive, the hash is
/// recomputed from the context's bytes and a disagreement is a refusal: the pair is then
/// internally inconsistent, and silently preferring either one would let a caller pin the
/// tokenizer of one job and the root of another — the exact substitution the binding exists to
/// close.
fn read_material(
    answer_path: &std::path::Path,
    dsl_path: Option<&std::path::Path>,
    job_context_path: Option<&std::path::Path>,
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
    let supplied_hash = match ctx_hex {
        Some(h) => Some(
            h.trim().parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("job context hash '{h}' is not 128-hex")))?,
        ),
        None => None,
    };
    // The whole context: `--job-context <file>` (borsh, or the same bytes as hex text), else the
    // gateway's own `job_context` field, which is that borsh as hex.
    let job_context = match job_context_path {
        Some(p) => Some(read_job_context_file(p)?),
        None => match block.get("job_context").and_then(|v| v.as_str()) {
            Some(hexed) => Some(decode_job_context(&from_hex(hexed, "the response's job_context")?, answer_path)?),
            None => None,
        },
    };
    let job_context_hash = match (&job_context, supplied_hash) {
        (Some(context), Some(supplied)) if context.context_hash() != supplied => {
            return Err(CliError::new(
                exit::GENERIC,
                format!(
                    "the job context and the job context hash you supplied are not the same job: the context hashes to {}, \
                     the hash beside it is {supplied}. One of the two belongs to another inference, and choosing either \
                     would let the tokenizer come from one job and the output_root from another — which is exactly what \
                     the binding exists to refuse. Pass the context alone (its hash is derived from it).",
                    context.context_hash()
                ),
            ));
        }
        // DERIVED, never taken beside the context — a caller who could pass both could pass one
        // that matches the tokenizer they wanted.
        (Some(context), _) => Some(context.context_hash()),
        (None, supplied) => supplied,
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
    Ok(ConsumerMaterial { output_token_ids: ids, job_context, job_context_hash, family, answer })
}

fn from_hex(text: &str, what: &str) -> Result<Vec<u8>, CliError> {
    let trimmed = text.trim();
    let mut raw = vec![0u8; trimmed.len() / 2];
    if !trimmed.len().is_multiple_of(2) || faster_hex::hex_decode(trimmed.as_bytes(), &mut raw).is_err() {
        return Err(CliError::new(exit::GENERIC, format!("{what} is not hex")));
    }
    Ok(raw)
}

fn decode_job_context(raw: &[u8], from: &std::path::Path) -> Result<PalwJobContextV2, CliError> {
    borsh::from_slice(raw).map_err(|e| {
        CliError::new(exit::GENERIC, format!("{}: the job context does not decode as a borsh PalwJobContextV2: {e}", from.display()))
    })
}

/// A `PalwJobContextV2` as a file — the borsh bytes, or the same bytes as hex text, the two forms
/// `palw-derive verify --job-context` accepts, so a consumer keeps one file for both tools.
fn read_job_context_file(path: &std::path::Path) -> Result<PalwJobContextV2, CliError> {
    let bytes = std::fs::read(path).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", path.display())))?;
    if let Ok(context) = borsh::from_slice::<PalwJobContextV2>(&bytes) {
        return Ok(context);
    }
    let text = String::from_utf8_lossy(&bytes);
    if let Ok(raw) = from_hex(&text, "the file")
        && let Ok(context) = borsh::from_slice::<PalwJobContextV2>(&raw)
    {
        return Ok(context);
    }
    Err(CliError::new(exit::GENERIC, format!("{} is not a borsh PalwJobContextV2 (nor the same bytes as hex text)", path.display())))
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

/// **Everything the consumer brought, in the two shapes that answer two different questions.**
///
/// `Unbound` is what this command could do before the binding existed: `dsl_hash` and
/// `artifact_hash` recomputed from bytes the caller supplied, `output_root` from ids the caller
/// supplied, and NOTHING computing one from the other — `rendered_output_hash_v1` is not the join,
/// because on every shipped family it is a keyed hash of the *ids* and never reaches a byte of
/// text. Both legs can be true of inputs with nothing to do with each other, so a pass on this
/// shape is `consistent-given-the-supplied-answer` and never a bare `consistent`.
///
/// `Bound` renders the claim's ids under the tokenizer the claim pins and re-runs the derivation
/// over THOSE bytes, so the two legs share an input. An enum rather than a fifth optional
/// argument, because the shapes are mutually exclusive by construction: a caller cannot hand this
/// a binding AND an answer whose relationship to it is unstated.
pub(crate) enum Evidence<'a> {
    Unbound { answer: Option<&'a [u8]>, output_root_inputs: Option<(PalwRcFamilyV1, Hash64, &'a [u32])> },
    Bound(Binding<'a>),
}

/// The inputs that turn a verification into a statement about ONE inference.
///
/// `job_context` is the whole context and its hash is never carried beside it: `tokenizer_id` is a
/// field of it, so a caller who passed the hash separately could pin the tokenizer that suits them
/// and a root from somewhere else.
pub(crate) struct Binding<'a> {
    pub family: PalwRcFamilyV1,
    pub job_context: &'a PalwJobContextV2,
    pub tokenizer: &'a QwenTokenizer,
    pub opened_tokenizer_id: Hash64,
    pub output_token_ids: &'a [u32],
}

/// The four recomputations of `misaka_palw_derive::verify`, into the mismatch list. One place,
/// because the bound and the unbound path must report the same fields under the same names.
fn push_verification(out: &mut Vec<Mismatch>, object: &PalwDerivedArtifactV1, v: &misaka_palw_derive::Verification) {
    if !v.dsl_hash_matches {
        out.push(Mismatch { field: "dsl_hash", on_chain: object.dsl_hash.to_string(), recomputed: v.recomputed_dsl_hash.to_string() });
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
///    (`misaka_palw_derive::verify`, or `verify_bound` over the RENDERING of the claim's ids).
/// 3. `derived_id` — LAST, and it is a check on the READER rather than on the executor: the id is
///    total over the object, so a disagreement means the object this reader rebuilt is not the one
///    the chain accepted (a different network domain, or an executor key the claim's bond no
///    longer holds), not that the derivation is false.
pub(crate) fn compare(
    object: &PalwDerivedArtifactV1,
    chain_derived_id: Hash64,
    evidence: &Evidence<'_>,
) -> Result<Vec<Mismatch>, String> {
    let mut out = Vec::new();
    match evidence {
        Evidence::Bound(b) => {
            // **`supplied_answer` is deliberately `None` here.** The gateway's response carries
            // `derivation.dsl`, which is the CANONICAL dsl — the grammar's normal form of the
            // answer, not the answer. `verify_bound`'s last leg compares the bytes it is handed to
            // the raw rendering, so passing the canonical form would report
            // `supplied_answer_is_the_rendering: false` for an honest derivation whose grammar
            // normalises anything at all, and this command would print MISMATCH at the executor.
            // Nothing is lost: `dsl_hash_matches` below is already computed over the rendering, so
            // it says the rendering IS the derivation's preimage, which is the whole binding.
            let v = misaka_palw_derive::verify_bound(
                object,
                b.family,
                b.job_context,
                b.tokenizer,
                b.opened_tokenizer_id,
                b.output_token_ids,
                None,
            )
            .map_err(|e| e.to_string())?;
            if !v.output_root_matches {
                out.push(Mismatch {
                    field: "output_root",
                    on_chain: object.output_root.to_string(),
                    recomputed: v.recomputed_output_root.to_string(),
                });
            }
            push_verification(&mut out, object, &v.verification);
        }
        Evidence::Unbound { answer, output_root_inputs } => {
            if let Some((family, job_context_hash, ids)) = output_root_inputs {
                let recomputed = misaka_palw_derive::recompute_output_root(*family, job_context_hash, ids);
                if recomputed != object.output_root {
                    out.push(Mismatch {
                        field: "output_root",
                        on_chain: object.output_root.to_string(),
                        recomputed: recomputed.to_string(),
                    });
                }
            }
            if let Some(answer) = answer {
                // A transformer this build does not publish is SA-5's case: the reader cannot
                // verify and says so, rather than reporting a pass it did not earn.
                let v = misaka_palw_derive::verify(object, answer).map_err(|e| e.to_string())?;
                push_verification(&mut out, object, &v);
            }
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

/// What `misaka palw derived-verify` was invoked with. A struct because the binding needs two
/// more inputs than the unbound check did and nine positional arguments is a call nobody can read
/// — the same shape `palw_court::CourtCloseArgs` already uses.
pub struct DerivedVerifyArgs<'a> {
    pub claim_id: &'a str,
    pub answer: &'a std::path::Path,
    pub dsl: Option<&'a std::path::Path>,
    /// The whole `PalwJobContextV2` (borsh, or the same bytes as hex text). The gateway's response
    /// carries it inline as `misaka.job_context`, so this is the override, not the usual route.
    pub job_context: Option<&'a std::path::Path>,
    pub job_context_hash: Option<&'a str>,
    /// The tokenizer file the class was converted from — the fourth binding input.
    pub tokenizer: Option<&'a std::path::Path>,
    pub family: Option<&'a str>,
    pub json: bool,
}

pub async fn verify(ctx: &Ctx, args: DerivedVerifyArgs<'_>) -> CliResult {
    let DerivedVerifyArgs { claim_id, answer: answer_path, dsl: dsl_path, json, .. } = args;
    let claim = parse_claim_id(claim_id)?;
    let material = read_material(answer_path, dsl_path, args.job_context, args.job_context_hash, args.family)?;
    // The tokenizer file, opened before the network call so a wrong path costs nothing.
    let tokenizer_bytes = match args.tokenizer {
        Some(p) => Some(std::fs::read(p).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", p.display())))?),
        None => None,
    };
    let tokenizer =
        match (&tokenizer_bytes, args.tokenizer) {
            (Some(bytes), Some(p)) => Some(QwenTokenizer::from_json(bytes).map_err(|e| {
                CliError::new(exit::GENERIC, format!("--tokenizer {}: not a readable tokenizer.json: {e}", p.display()))
            })?),
            _ => None,
        };
    let opened_tokenizer_id = tokenizer_bytes.as_deref().map(misaka_palw_derive::opened_tokenizer_id_v1);
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

    // ------------------------------------------------------------------------------------
    // The binding: four inputs, and the tokenizer pin checked BEFORE anything is rendered.
    // ------------------------------------------------------------------------------------
    let missing: Vec<&str> = [
        (material.output_token_ids.is_none(), "output_token_ids (in the gateway response, or a bare JSON array)"),
        (material.job_context.is_none(), "the full job context (`misaka.job_context`, or --job-context <file>)"),
        (tokenizer.is_none(), "--tokenizer <tokenizer.json>"),
        (material.family.is_none(), "the family (--family, or the gateway response's)"),
    ]
    .into_iter()
    .filter_map(|(absent, name)| absent.then_some(name))
    .collect();
    let evidence = match (&material.job_context, &tokenizer, opened_tokenizer_id, material.family, material.output_token_ids.as_ref())
    {
        (Some(job_context), Some(tok), Some(opened), Some(family), Some(ids)) => {
            // **Refused here rather than left to `verify_bound`'s Err.** A tokenizer that is not
            // the one the claim pins is the CALLER holding the wrong file; reporting it down the
            // MISMATCH road would file the reader's own mistake as a demonstrable false object.
            misaka_palw_derive::check_tokenizer_pin_v1(job_context, opened)
                .map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?;
            Evidence::Bound(Binding { family, job_context, tokenizer: tok, opened_tokenizer_id: opened, output_token_ids: ids })
        }
        _ => Evidence::Unbound { answer: material.answer.as_deref(), output_root_inputs },
    };
    let binding_checked = matches!(evidence, Evidence::Bound(_));
    let as_json = json || ctx.output == OutputFormat::Json;
    // **Two different failures, counted apart.** A row that disagrees is the executor's fault; a
    // row this build cannot re-run is the READER's version, and folding them into one flag is how
    // the summary line below came to call an honest executor a forger. See `overall_verdict`.
    let mut any_mismatch = false;
    let mut any_unverifiable = false;
    let mut rows = Vec::new();
    for row in &response.artifacts {
        let (object, chain_derived_id) = object_from_chain(&response, row, reader.network_domain)?;
        let name = kind::name(object.kind).unwrap_or("?");
        let verdict = match compare(&object, chain_derived_id, &evidence) {
            Ok(mismatches) if mismatches.is_empty() => {
                let checked = describe_checked(material.answer.is_some(), output_root_inputs.is_some(), binding_checked);
                let word = if binding_checked { "consistent" } else { "consistent-given-the-supplied-answer" };
                serde_json::json!({ "transformer_id": row.transformer_id, "kind": row.kind, "kind_name": name, "verdict": word, "binding_checked": binding_checked, "checked": checked })
            }
            Ok(mismatches) => {
                any_mismatch = true;
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
                // It is not, however, a falsehood — see `overall_verdict`.
                any_unverifiable = true;
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
    // The binding is a property of the claim and the consumer's material, not of one derivation
    // row, so it is stated once — with the numbers that say what was actually rendered.
    let binding_note = match &evidence {
        Evidence::Bound(b) => {
            let rendered = misaka_palw_derive::render_answer_v1(b.tokenizer, b.output_token_ids);
            let tokenizer_id = b.opened_tokenizer_id.to_string();
            serde_json::json!({
                "binding_checked": true,
                "binding": format!(
                    "dsl_hash, artifact_hash and artifact_bytes were recomputed over the bytes this claim's {} ids RENDER \
                     to under the tokenizer it pins ({}…), and output_root over those same ids — not over an answer \
                     supplied beside them. So `consistent` here is a statement about ONE inference.",
                    b.output_token_ids.len(),
                    short(&tokenizer_id),
                ),
                "tokenizer_id": tokenizer_id,
                "job_context_hash": b.job_context.context_hash().to_string(),
                "rendered_answer_bytes": rendered.len(),
                // The DSL the response carried was NOT used as the derivation's preimage: it is
                // the grammar's canonical form, and comparing it to the raw rendering would call
                // an honest executor a forger over the grammar's own normalisation.
                "supplied_answer_used": false,
            })
        }
        Evidence::Unbound { .. } => serde_json::json!({
            "binding_checked": false,
            "binding_not_checked_because": format!(
                "missing {}. Without all four, nothing here computes the answer FROM the claim's ids: `dsl_hash` and \
                 `artifact_hash` are recomputed from bytes you supplied and `output_root` from ids you supplied, and the \
                 two never meet (`rendered_output_hash_v1` hashes the IDS, not the rendered text). An executor can attach \
                 any artifact of any kind to any of its own claims and pass every check on this path.",
                missing.join(", ")
            ),
        }),
    };
    let mut document = serde_json::json!({
        "schema": "misaka.palw.chain-derive-verify.v1",
        "claim_id": response.claim_id,
        "claim_phase": response.claim_phase,
        "claim_void_reason": response.claim_void_reason,
        "output_root": response.output_root,
        "executor_bond": response.executor_bond,
        "derivations": rows,
        "verdict": overall_verdict(any_mismatch, any_unverifiable, binding_checked),
    });
    for (k, v) in binding_note.as_object().expect("an object").clone() {
        document.as_object_mut().expect("an object").insert(k, v);
    }
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
        // The sentence a reader needs BEFORE the verdict word, because it is what the word means.
        for key in ["binding", "binding_not_checked_because"] {
            if let Some(sentence) = document.get(key).and_then(|v| v.as_str()) {
                println!("  {key}: {sentence}");
            }
        }
        println!("{}", document["verdict"].as_str().unwrap_or(""));
    }
    match (any_mismatch, any_unverifiable) {
        (false, false) => Ok(()),
        (true, _) => Err(CliError::new(exit::GENERIC, "the chain and the recomputation disagree")),
        // Nonzero, because a reader who could not check an object must not treat it as checked —
        // but the sentence is about this build, not about the executor.
        (false, true) => Err(CliError::new(
            exit::GENERIC,
            "this build could not re-run the derivation, so nothing was checked either way (ADR-0078 SA-5)",
        )),
    }
}

/// **The summary line, and the one place the two failures must not be confused.**
///
/// `MISMATCH` is ADR-0078 Decision 5's word: the executor filed a derivation that re-running the
/// named grammar and transformer over the answer refutes, and the whole value of the sentence is
/// that it is an accusation somebody can act on. `UNVERIFIABLE` is SA-5's: `transformer_id` covers
/// `misaka-palw-derive/src/`, so any edit there moves all eight ids and this build simply is not
/// the one the derivation names. Nobody lied; the reader is holding the wrong ruler.
///
/// This summary used to be `if all_consistent { .. } else { "MISMATCH — a demonstrable false
/// object" }` over a single flag, which meant a claim whose every row was `UNVERIFIABLE` still
/// printed a forgery accusation as its LAST line — the line a human reads and a script greps.
/// `palw-derive verify` was fixed for exactly this; the command a launch note points strangers at
/// was not.
///
/// A mismatch alongside an unverifiable row still reads MISMATCH: one refuted row is a refutation
/// whatever else could not be checked.
///
/// **And a pass says whether it BOUND anything.** `consistent` used to be printed for a run where
/// `dsl_hash` came from bytes the caller supplied and `output_root` from ids the caller supplied,
/// with nothing computing one from the other — a sentence a reader takes as "this artifact came
/// from that inference" and which was never that. The unbound word is
/// `consistent-given-the-supplied-answer`, which is true; the bare `consistent` is now only
/// printed when the DSL was RENDERED from the claim's own ids.
fn overall_verdict(any_mismatch: bool, any_unverifiable: bool, binding_checked: bool) -> &'static str {
    match (any_mismatch, any_unverifiable, binding_checked) {
        (false, false, true) => {
            "consistent — binding_checked: true; the derivation's DSL is the rendering of this claim's ids under the tokenizer the claim pins (ADR-0078 X6)"
        }
        (false, false, false) => {
            "consistent-given-the-supplied-answer — binding_checked: false; NOT a statement that this artifact came from that inference (see binding_not_checked_because)"
        }
        (true, _, _) => "MISMATCH — a demonstrable false object (ADR-0078 Decision 5)",
        (false, true, _) => "UNVERIFIABLE — this build does not publish that manifest (ADR-0078 SA-5)",
    }
}

/// A pass has to say what it covered: "consistent" over one of the FOUR recomputations is not the
/// same statement as "consistent" over all of them, and a reader who cannot tell them apart has
/// been given a verdict they cannot use.
///
/// The fourth is the binding — whether the DSL was the RENDERING of the claim's ids — and it is
/// the one people read the command for. Without it the other three are true of the bytes the
/// caller happened to pass, so the `(true, true)` row below no longer says "in full".
fn describe_checked(reran: bool, output_root: bool, bound: bool) -> &'static str {
    if bound {
        return "the DSL IS the rendering of this claim's ids (the binding), output_root, dsl_hash, artifact_hash, artifact_bytes, kind, derived_id (ADR-0078 X6, in full)";
    }
    match (reran, output_root) {
        (true, true) => {
            "output_root, dsl_hash, artifact_hash, artifact_bytes, kind, derived_id — NOT the binding: the first came from ids you supplied and the rest from bytes you supplied, and nothing computed either from the other (pass --tokenizer, with a response carrying `job_context`)"
        }
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
    use kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2;
    use misaka_palw_derive::{ClaimBinding, derive_named, opened_tokenizer_id_v1, recompute_output_root, render_answer_v1};

    const MUSIC_TRANSFORMER: &str = "music/smf/v1";

    /// The shape this command could always do: two true sentences about two unrelated inputs.
    fn unbound<'a>(answer: &'a [u8], family: PalwRcFamilyV1, job_context_hash: Hash64, ids: &'a [u32]) -> Evidence<'a> {
        Evidence::Unbound { answer: Some(answer), output_root_inputs: Some((family, job_context_hash, ids)) }
    }

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
        let mismatches = compare(&derivation.object, chain_derived_id, &unbound(&answer, family, job_context_hash, &ids))
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
            compare(&lie, chain_derived_id, &unbound(&answer, family, job_context_hash, &ids)).expect("still re-runnable");
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
        let mismatches =
            compare(&derivation.object, chain_derived_id, &unbound(&answer, family, job_context_hash, &other)).expect("re-runnable");
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
        let mismatches = compare(&rebuilt, chain_derived_id, &unbound(&answer, family, job_context_hash, &ids)).expect("re-runnable");
        assert_eq!(mismatches.iter().map(|m| m.field).collect::<Vec<_>>(), vec!["derived_id"]);
    }

    /// Without the answer bytes and without the ids, the only thing left to check is the
    /// reconstruction — and the verdict says exactly that rather than reporting a bare pass.
    ///
    /// The fourth recomputation is the BINDING, and it is why the `(true, true)` row that used to
    /// read "in full" no longer does when nothing bound the artifact to the claim.
    #[test]
    fn a_pass_says_which_of_the_four_recomputations_it_covered() {
        // The FOURTH: only a bound run may say "in full", because without the binding the other
        // three are true of the bytes the caller happened to pass.
        assert!(describe_checked(true, true, true).contains("in full"));
        assert!(describe_checked(true, true, true).contains("the rendering of this claim's ids"));
        assert!(!describe_checked(true, true, false).contains("in full"));
        assert!(describe_checked(true, true, false).contains("NOT the binding"));
        assert!(describe_checked(true, false, false).contains("NOT output_root"));
        assert!(describe_checked(false, true, false).contains("NOT the derivation"));
        assert!(describe_checked(false, false, false).contains("nothing about this derivation was checked"));
    }

    /// **A derivation this build cannot re-run is not a derivation this build has refuted.**
    ///
    /// `transformer_id` names the source tree that produced the artifact, so a node upgrade that
    /// touches `misaka-palw-derive/src/` moves all eight ids and every derivation already on chain
    /// becomes one THIS build cannot check. `compare` reports that as `Err` — SA-5 — and the row
    /// is rendered `UNVERIFIABLE`. This is the fixture the summary-line test below stands on: if
    /// `compare` ever started returning `Ok(vec![...])` here, `UNVERIFIABLE` would silently become
    /// `MISMATCH` and an honest executor would be accused by an upgrade.
    #[test]
    fn a_transformer_this_build_does_not_publish_is_unverifiable_and_not_a_mismatch() {
        let ids = vec![7u32, 11, 13];
        let (binding, family, job_context_hash) = binding(&ids);
        let answer = corpus_answer();
        let derivation = derive_named(MUSIC_TRANSFORMER, &binding, &answer).expect("the shipped corpus derives");

        // The object an OLDER build filed: everything true, under an id this tree never published.
        let mut stale = derivation.object.clone();
        stale.transformer_id = h(0x5A);
        let chain_derived_id = derived_id_v1(&stale);
        let why = compare(&stale, chain_derived_id, &unbound(&answer, family, job_context_hash, &ids))
            .expect_err("a transformer this build does not publish cannot be re-run");
        assert!(why.contains("transformer"), "the refusal does not name what is unknown: {why}");
    }

    /// **The summary line must not call an honest executor a forger.**
    ///
    /// The last line `derived-verify` prints — and the string a script greps — is this one. It was
    /// derived from a single `all_consistent` flag, so a claim whose every row was `UNVERIFIABLE`
    /// printed "a demonstrable false object" underneath them. The three cases are distinct
    /// sentences and this pins all three.
    #[test]
    fn the_summary_line_separates_unverifiable_from_a_false_object() {
        // A pass says whether it BOUND anything, and the unbound word is the qualified one.
        assert_eq!(
            overall_verdict(false, false, true),
            "consistent — binding_checked: true; the derivation's DSL is the rendering of this claim's ids under the tokenizer the claim pins (ADR-0078 X6)"
        );
        assert_eq!(
            overall_verdict(false, false, false),
            "consistent-given-the-supplied-answer — binding_checked: false; NOT a statement that this artifact came from that inference (see binding_not_checked_because)"
        );
        // A script that greps for a pass still finds one on both, and a script that reads the
        // whole word can tell them apart — which is the point.
        assert!(overall_verdict(false, false, true).starts_with("consistent"));
        assert!(overall_verdict(false, false, false).starts_with("consistent"));
        assert!(!overall_verdict(false, false, false).starts_with("consistent —"));
        // The regression: nothing was refuted, so nothing may be called false.
        let unverifiable = overall_verdict(false, true, false);
        assert!(unverifiable.starts_with("UNVERIFIABLE"), "{unverifiable}");
        assert!(!unverifiable.contains("false object"), "an unverifiable claim is being called a forgery: {unverifiable}");
        assert!(overall_verdict(true, false, true).contains("false object"));
        // One refuted row is a refutation whatever else could not be checked.
        assert!(overall_verdict(true, true, false).contains("false object"));
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
        let material = read_material(&path, None, None, None, None).expect("the gateway response reads");
        assert_eq!(material.output_token_ids.as_deref(), Some([7u32, 11, 13].as_slice()));
        assert_eq!(material.job_context_hash, Some(h(0xC7)));
        assert_eq!(material.family, Some(PalwRcFamilyV1::Base0));
        assert_eq!(material.answer.as_deref(), Some(b"{\"v\":1}".as_slice()));
        // A hash-only response is the OLD gateway's, and it cannot bind: no context, no tokenizer
        // pin, and the verdict word stays the qualified one.
        assert_eq!(material.job_context, None);
        let _ = std::fs::remove_file(&path);
    }

    // ------------------------------------------------------------------------------------
    // The binding
    // ------------------------------------------------------------------------------------

    /// A job context that pins one tokenizer. Every other field is fixed nonsense: the binding is
    /// a statement about `tokenizer_id` and `context_hash()`, and the rest is here only because
    /// those two are functions of all of it.
    fn job_context(tokenizer_id: Hash64) -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-cli-derived-verify".to_vec(),
            job_id: h(0x11),
            job_nullifier: h(0x12),
            assignment_id: h(0x13),
            execution_seed: [0x14; 32],
            model_profile_id: h(0x15),
            runtime_manifest_hash: h(0x16),
            runtime_class_id: h(0x17),
            shape_profile_id: h(0x18),
            trace_scheme_id: h(0x19),
            cu_ruleset_id: h(0x1a),
            tokenizer_id,
            prompt_token_ids_hash: h(0x1b),
            declared_prefill_tokens: 8,
            exact_decode_tokens: 64,
            max_context_tokens: 512,
        }
    }

    /// **A tokenizer whose ids ARE chunks of one text**, as a `tokenizer.json` the real
    /// `QwenTokenizer::from_json` reads. Added tokens are matched whole and `token_bytes` returns
    /// their content verbatim, so the concatenation over the ids is the text — the same
    /// `render_answer_v1` the gateway runs, without a 7 MB fixture. The test below uses a real
    /// tokenizer file when the machine has one.
    fn tokenizer_over(text: &[u8], chunk: usize) -> (Vec<u8>, Vec<u32>) {
        let text = std::str::from_utf8(text).expect("the corpus is UTF-8");
        let mut added = Vec::new();
        let mut ids = Vec::new();
        let mut start = 0usize;
        while start < text.len() {
            let mut end = (start + chunk).min(text.len());
            while !text.is_char_boundary(end) {
                end += 1;
            }
            let id = (added.len() + 1) as u32;
            added.push(serde_json::json!({ "id": id, "content": &text[start..end], "special": false }));
            ids.push(id);
            start = end;
        }
        let json = serde_json::json!({ "model": { "vocab": { "a": 0 }, "merges": [] }, "added_tokens": added });
        (serde_json::to_vec(&json).expect("serializable"), ids)
    }

    /// One honest run over a tokenizer file: the ids, the context that pins it, the answer those
    /// ids render to, and a real derivation of it filed against the claim those ids imply.
    struct Honest {
        tokenizer: QwenTokenizer,
        opened: Hash64,
        ids: Vec<u32>,
        ctx: PalwJobContextV2,
        answer: Vec<u8>,
        object: PalwDerivedArtifactV1,
        derived_id: Hash64,
    }

    fn honest_over(tokenizer_bytes: &[u8], ids: Vec<u32>) -> Honest {
        let tokenizer = QwenTokenizer::from_json(tokenizer_bytes).expect("a readable tokenizer.json");
        let opened = opened_tokenizer_id_v1(tokenizer_bytes);
        let ctx = job_context(opened);
        let answer = render_answer_v1(&tokenizer, &ids);
        let family = PalwRcFamilyV1::Base0;
        let claim = ClaimBinding {
            network_domain: h(0x11),
            claim_id: h(0xC1),
            output_root: recompute_output_root(family, &ctx.context_hash(), &ids),
            executor_pubkey: vec![0xAB; 2592],
        };
        let derivation = derive_named(MUSIC_TRANSFORMER, &claim, &answer).expect("the rendered answer derives");
        let derived_id = derived_id_v1(&derivation.object);
        Honest { tokenizer, opened, ids, ctx, answer, object: derivation.object, derived_id }
    }

    fn bound(honest: &Honest) -> Evidence<'_> {
        Evidence::Bound(Binding {
            family: PalwRcFamilyV1::Base0,
            job_context: &honest.ctx,
            tokenizer: &honest.tokenizer,
            opened_tokenizer_id: honest.opened,
            output_token_ids: &honest.ids,
        })
    }

    /// **The two shapes, over one honest derivation and one forgery.**
    ///
    /// The forgery is the hole this whole change exists to close, executed at the CLI's own
    /// comparison: one honest claim, an artifact derived from a COMPLETELY DIFFERENT answer, filed
    /// against that claim with the claim's own `output_root` — the two fields the chain checks.
    /// The unbound shape reports no mismatch at all, which is why its verdict word may not be a
    /// bare `consistent`; the bound shape renders the claim's ids and names `dsl_hash` and
    /// `artifact_hash` while `output_root` stays true.
    #[test]
    fn the_bound_shape_catches_the_forgery_the_unbound_shape_passes() {
        let answer = corpus_answer();
        let (tokenizer_bytes, ids) = tokenizer_over(&answer, 24);
        let honest = honest_over(&tokenizer_bytes, ids);
        assert_eq!(honest.answer, answer, "the fixture tokenizer's round trip must be the identity");

        // Honest: both shapes agree, and the bound one is the statement about one inference.
        assert!(
            compare(&honest.object, honest.derived_id, &bound(&honest)).expect("re-runnable").is_empty(),
            "a true derivation of the claim's own rendering disagrees with the chain about nothing"
        );
        assert!(
            compare(
                &honest.object,
                honest.derived_id,
                &unbound(&honest.answer, PalwRcFamilyV1::Base0, honest.ctx.context_hash(), &honest.ids)
            )
            .expect("re-runnable")
            .is_empty()
        );

        // The forgery: an artifact of another answer, filed against THIS claim.
        let other =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-derive/corpus/music/03-overlapping-melody.json");
        let other = std::fs::read(&other).unwrap_or_else(|e| panic!("{}: {e}", other.display()));
        assert_ne!(other, honest.answer, "the forgery must be derived from a different answer");
        let claim = ClaimBinding {
            network_domain: h(0x11),
            claim_id: h(0xC1),
            // The claim's real root — unchanged, because a forger does not need to touch it.
            output_root: honest.object.output_root,
            executor_pubkey: vec![0xAB; 2592],
        };
        let forged = derive_named(MUSIC_TRANSFORMER, &claim, &other).expect("the other answer derives");
        let forged_id = derived_id_v1(&forged.object);

        // The old conjunction, true of the forgery: the caller was handed the answer the artifact
        // really came from, beside the ids the claim really committed.
        assert!(
            compare(&forged.object, forged_id, &unbound(&other, PalwRcFamilyV1::Base0, honest.ctx.context_hash(), &honest.ids))
                .expect("re-runnable")
                .is_empty(),
            "this is the defect: every check passes on the unbound shape"
        );
        // And the binding, which renders the claim's OWN ids and re-runs over those bytes.
        let mismatches = compare(&forged.object, forged_id, &bound(&honest)).expect("re-runnable");
        let fields: Vec<&str> = mismatches.iter().map(|m| m.field).collect();
        assert!(fields.contains(&"dsl_hash"), "{fields:?}");
        assert!(fields.contains(&"artifact_hash"), "{fields:?}");
        assert!(!fields.contains(&"output_root"), "the forger did not have to touch the root: {fields:?}");
    }

    /// **A tokenizer the claim does not pin is refused BY NAME, not called a forgery.**
    ///
    /// The caller is holding the wrong file; filing that under "a demonstrable false object" would
    /// accuse the executor of the reader's own mistake. `verify` checks the pin before it renders
    /// anything, so this is the refusal `check_tokenizer_pin_v1` writes.
    #[test]
    fn a_tokenizer_the_claim_does_not_pin_is_refused_by_name() {
        let answer = corpus_answer();
        let (tokenizer_bytes, ids) = tokenizer_over(&answer, 24);
        let honest = honest_over(&tokenizer_bytes, ids);
        let (other_bytes, _) = tokenizer_over(&answer, 17);
        let why = misaka_palw_derive::check_tokenizer_pin_v1(&honest.ctx, opened_tokenizer_id_v1(&other_bytes))
            .expect_err("a tokenizer that is not the pinned one cannot render this claim's ids");
        let why = why.to_string();
        assert!(why.contains(&honest.opened.to_string()), "the refusal names the pin: {why}");
        assert!(why.contains("tokenizer_id_v2_for_gguf"), "and the lineage it cannot check from a file: {why}");
    }

    /// The same binding through a REAL tokenizer file, when this machine has one.
    ///
    /// Skipped BY NAME when it does not — never a pass by absence: the message says which paths
    /// were looked for, so a green run on a machine without them cannot be mistaken for the check
    /// having happened. The synthetic-tokenizer test above still runs everywhere, so what this
    /// adds is the real byte-level lineage: `opened_tokenizer_id_v1` over a shipped
    /// `tokenizer.json`, and ids the tokenizer itself produced.
    #[test]
    fn the_binding_holds_through_a_real_tokenizer_file() {
        const REAL_TOKENIZERS: &[&str] = &[
            "/Users/wata/Downloads/qwen25-tokenizer.json",
            "/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/71440f68-0f3b-4144-8b20-73c6aae7fb86/scratchpad/qwen35-2b-tokenizer.json",
        ];
        let Some(path) = REAL_TOKENIZERS.iter().map(std::path::Path::new).find(|p| p.exists()) else {
            eprintln!(
                "SKIPPED the_binding_holds_through_a_real_tokenizer_file: no real tokenizer.json present. Looked for {}. \
                 The dense class artifacts these pair with are \
                 /private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/71440f68-0f3b-4144-8b20-73c6aae7fb86/scratchpad/instruct-bound.palwart \
                 and /Users/wata/Downloads/qwen25-1.5b-a16.palwart. This check did NOT run.",
                REAL_TOKENIZERS.join(", ")
            );
            return;
        };
        let bytes = std::fs::read(path).expect("the tokenizer file");
        let tokenizer = QwenTokenizer::from_json(&bytes).expect("a readable tokenizer.json");
        let answer = corpus_answer();
        let text = std::str::from_utf8(&answer).expect("UTF-8");
        let ids = tokenizer.encode_without_specials(text).expect("the corpus answer tokenizes");
        assert_eq!(
            render_answer_v1(&tokenizer, &ids),
            answer,
            "a real tokenizer's round trip must be the identity, or this test is measuring the tokenizer"
        );
        let honest = honest_over(&bytes, ids);
        assert!(
            compare(&honest.object, honest.derived_id, &bound(&honest)).expect("re-runnable").is_empty(),
            "the binding holds over {}",
            path.display()
        );
        assert_ne!(honest.opened, Hash64::default(), "a real tokenizer file commits to something");
        eprintln!("the_binding_holds_through_a_real_tokenizer_file: bound through {}", path.display());
    }
}
