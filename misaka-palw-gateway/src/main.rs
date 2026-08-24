//! `misaka-palw-gateway` — the free-prompt front end (ADR-0044 Decision 10, FP-07).
//!
//! ```text
//! user app ──POST /v1/chat/completions──▶ this process
//!     │  canonical template render (a frozen string transform, HERE — never in the worker)
//!     ▼
//! palw-worker --mode v3-job (Text arm) ──▶ the ANSWER, streamed back as an OpenAI-style reply
//!                                     └──▶ the commitment inputs (trace/output/schedule roots),
//!                                          written to the outbox for the executor rail
//! ```
//!
//! **One inference.** The gateway never re-runs the model for mining — there is no second lane,
//! and nothing here can create one: the worker result carries both the answer and the roots, and
//! the caller-side re-binding (`validate_against_request`) is the same discipline the agent
//! client uses — the worker is never trusted about what it was asked.
//!
//! **F1 lives here.** The bytes handed to the model are exactly the canonical template over the
//! user's messages: no DAA suffix, no job metadata, no mining fields. Chain binding (anchor,
//! nonce, bond) rides in the job identity, outside the token stream, and the unit tests pin the
//! rendered form.
//!
//! **What the outbox holds, honestly.** The framed `PalwFpWorkerResultV3`, the UNSIGNED
//! `PalwFreePromptCommitmentV3` (with the real retained-trace DA trio — the worker chunks the
//! ordered event-hash list to `<outbox>/traces/<job-id>/` before its result frame exists), and
//! a JSON summary. The gateway does NOT fabricate the one piece it must not have: the ML-DSA
//! signature belongs to the signer sidecar, and the summary names that and the transaction
//! rail as the two remaining steps.
//!
//! **HTTP, hand-rolled.** One POST route and a health probe over std's `TcpListener`, following
//! `rpc/eth`'s in-tree precedent of not pulling an async HTTP stack for a small, exact surface.
//! Non-streaming by design at v1 (the commitment only exists at completion; a token stream is a
//! side channel for a later revision — ADR-0044 "Not decided").

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpCuWeightsV3, PalwFpStopReasonV3, PalwFpWorkerInputV3, PalwFpWorkerRequestV3,
    PalwFpWorkerResultV3, fp_job_id_v3, fp_quanta_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, read_framed, write_framed};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use serde::Deserialize;

// ---------------------------------------------------------------------------------------------
// The canonical chat template, v1 — a frozen transform. Its identity is part of the class
// profile (ADR-0044 Decision 10); editing it in place is a fork of the class, so: don't.
//
// Plain-text markers, deliberately. The pinned tokenizer runs with `parse_special = false`
// (untrusted text must never smuggle control tokens), so a ChatML template would tokenize its
// own markers as ordinary text — the worst of both. A ChatML profile with segment-wise special
// tokenization is a FUTURE class profile, not an edit of this one. Consequence, stated: the
// model rarely emits EOG under this template, so answers end at the decode ceiling and the
// present-layer stop-trim below handles display; the commitment always covers the full executed
// output.
// ---------------------------------------------------------------------------------------------
const TEMPLATE_ID_V1: &str = "misaka-palw/fp-gateway-template/plain-markers/v1";
const MARKER_SYSTEM: &str = "### System:\n";
const MARKER_USER: &str = "### User:\n";
const MARKER_ASSISTANT: &str = "### Assistant:\n";
const TURN_SEPARATOR: &str = "\n\n";
/// The display-layer stop guard: the first occurrence of a fresh marker line ends the SHOWN
/// answer. Presentation only — the commitment covers every executed token.
const STOP_GUARD: &str = "\n###";

fn render_template_v1(messages: &[ChatMessage]) -> Result<String, String> {
    if !messages.iter().any(|m| m.role == "user") {
        return Err("the request carries no user message".into());
    }
    let mut out = String::new();
    for message in messages {
        let marker = match message.role.as_str() {
            "system" => MARKER_SYSTEM,
            "user" => MARKER_USER,
            "assistant" => MARKER_ASSISTANT,
            other => return Err(format!("unsupported role {other:?} (system|user|assistant)")),
        };
        out.push_str(marker);
        out.push_str(&message.content);
        out.push_str(TURN_SEPARATOR);
    }
    out.push_str(MARKER_ASSISTANT);
    Ok(out)
}

/// Trim the SHOWN answer at the stop guard. The full text stays in the artifact.
fn display_trim(rendered: &str) -> &str {
    match rendered.find(STOP_GUARD) {
        Some(at) => rendered[..at].trim_end(),
        None => rendered.trim_end(),
    }
}

// ---------------------------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct IdentityFile {
    /// 64-byte hex: the network's domain separator — the same value the attempt lane binds.
    network_domain: String,
    /// 64-byte hex: the registered execution class this gateway's worker embodies.
    class_id: String,
    /// 64-byte hex transaction id of the executor bond outpoint.
    bond_txid: String,
    bond_index: u32,
    /// Hex: the bond's ML-DSA-87 public key (carried; the signer sidecar holds the secret).
    executor_pubkey: String,
    /// 64-byte hex: the operator identity registered with the bond.
    operator_id: String,
}

#[derive(Deserialize)]
struct AnchorFile {
    /// 64-byte hex: a recent chain block — the freshness binding. An external rail refreshes
    /// this file; the gateway re-reads it per request so a long-lived process never staples a
    /// stale anchor onto new work.
    anchor_block: String,
    anchor_daa: u64,
}

struct Identity {
    network_domain: Hash64,
    class_id: Hash64,
    executor_bond: TransactionOutpoint,
    executor_pubkey: Vec<u8>,
    operator_id: Hash64,
}

struct WorkerIdentity {
    model_profile_id: Hash64,
    runtime_manifest_hash: Hash64,
    runtime_class_id: Hash64,
    shape_profile_id: Hash64,
    trace_scheme_id: Hash64,
    prefill_cap: u32,
    n_ctx: u32,
}

struct Config {
    listen: String,
    worker: PathBuf,
    outbox: PathBuf,
    identity_path: PathBuf,
    anchor_path: PathBuf,
    cu_weights: PalwFpCuWeightsV3,
    /// Devnet display aid: the bundle's quantum size, so the summary can say how many draws a
    /// job earned. Zero disables the display (no bundle known).
    quantum_cu: u128,
    max_decode_default: u32,
    max_decode_cap: u32,
    /// How long past the job's anchor the producer promises to serve retained-trace chunks, in
    /// DAA score. A chain-time promise, so it rides the caller side of `to_commitment`.
    trace_retention_window_daa: u64,
}

fn die(msg: String) -> ! {
    eprintln!("[misaka-palw-gateway] fatal: {msg}");
    std::process::exit(1);
}

fn hex64(s: &str, what: &str) -> Hash64 {
    let mut out = [0u8; 64];
    if s.len() != 128 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not 128 hex chars"));
    }
    Hash64::from_bytes(out)
}

fn hex_bytes(s: &str, what: &str) -> Vec<u8> {
    if !s.len().is_multiple_of(2) {
        die(format!("{what} is not even-length hex"));
    }
    let mut out = vec![0u8; s.len() / 2];
    if faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not hex"));
    }
    out
}

fn load_identity(path: &Path) -> Identity {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("cannot read identity file {}: {e}", path.display())));
    let file: IdentityFile = serde_json::from_str(&raw).unwrap_or_else(|e| die(format!("identity file is not valid JSON: {e}")));
    let pubkey = hex_bytes(&file.executor_pubkey, "executor_pubkey");
    if pubkey.is_empty() {
        die("executor_pubkey is empty — an unaccountable gateway must not produce commitments".into());
    }
    Identity {
        network_domain: hex64(&file.network_domain, "network_domain"),
        class_id: hex64(&file.class_id, "class_id"),
        executor_bond: TransactionOutpoint {
            transaction_id: TransactionId::from_bytes(hex64(&file.bond_txid, "bond_txid").as_bytes()),
            index: file.bond_index,
        },
        executor_pubkey: pubkey,
        operator_id: hex64(&file.operator_id, "operator_id"),
    }
}

fn load_anchor(path: &Path) -> Result<(Hash64, u64), String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("cannot read anchor file {}: {e}", path.display()))?;
    let file: AnchorFile = serde_json::from_str(&raw).map_err(|e| format!("anchor file is not valid JSON: {e}"))?;
    let mut out = [0u8; 64];
    if file.anchor_block.len() != 128 || faster_hex::hex_decode(file.anchor_block.as_bytes(), &mut out).is_err() {
        return Err("anchor_block is not 128 hex chars".into());
    }
    Ok((Hash64::from_bytes(out), file.anchor_daa))
}

fn query_worker_identity(worker: &Path) -> WorkerIdentity {
    let output = Command::new(worker)
        .args(["--mode", "v3-manifest"])
        .output()
        .unwrap_or_else(|e| die(format!("cannot run {} --mode v3-manifest: {e}", worker.display())));
    if !output.status.success() {
        die(format!("worker v3-manifest failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let doc: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| die(format!("v3-manifest is not JSON: {e}")));
    let field = |k: &str| -> Hash64 { hex64(doc[k].as_str().unwrap_or_else(|| die(format!("v3-manifest lacks {k}"))), k) };
    WorkerIdentity {
        model_profile_id: field("model_profile_id"),
        runtime_manifest_hash: field("runtime_manifest_hash"),
        runtime_class_id: field("runtime_class_id"),
        shape_profile_id: field("shape_profile_id"),
        trace_scheme_id: field("trace_scheme_id"),
        prefill_cap: doc["prefill_single_batch_cap"].as_u64().unwrap_or(0) as u32,
        n_ctx: doc["n_ctx"].as_u64().unwrap_or(0) as u32,
    }
}

// ---------------------------------------------------------------------------------------------
// OpenAI-compatible request/response shapes (the subset this surface serves)
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    #[serde(default)]
    model: Option<String>,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    max_tokens: Option<u32>,
    /// Refused when true: v1 is non-streaming, and silently downgrading a stream request would
    /// make clients hang on SSE parsing.
    #[serde(default)]
    stream: Option<bool>,
}

// ---------------------------------------------------------------------------------------------
// The worker round trip
// ---------------------------------------------------------------------------------------------

fn run_worker_v3(worker: &Path, trace_out: &Path, request: &PalwFpWorkerRequestV3) -> Result<(PalwFpWorkerResultV3, Hash64), String> {
    let payload = borsh::to_vec(request).map_err(|e| format!("cannot serialize the worker request: {e}"))?;
    let request_hash = fp_worker_request_hash_v3(&payload);
    let mut child = Command::new(worker)
        .args(["--mode", "v3-job", "--trace-out", &trace_out.display().to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot spawn {}: {e}", worker.display()))?;

    // Drain stderr concurrently — a filled pipe buffer wedges the child (the live incident the
    // worker's own docs record); the log lines go to our stderr prefixed.
    let stderr = child.stderr.take().expect("piped");
    let drain = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("[palw-worker] {line}");
        }
    });

    {
        let mut stdin = child.stdin.take().expect("piped");
        write_framed(&mut stdin, &payload).map_err(|e| format!("cannot write the job frame: {e}"))?;
        // stdin drops here — EOF, the single-frame contract.
    }
    let mut stdout = child.stdout.take().expect("piped");
    let result_bytes = read_framed(&mut stdout, PALW_V2_MAX_FRAME_BYTES).map_err(|e| format!("worker produced no result frame: {e}"));
    let status = child.wait().map_err(|e| format!("cannot reap the worker: {e}"))?;
    let _ = drain.join();
    if !status.success() {
        return Err(format!("the worker refused the job (exit {status}) — see its log lines above"));
    }
    let result: PalwFpWorkerResultV3 =
        borsh::from_slice(&result_bytes?).map_err(|e| format!("the worker result frame does not decode: {e}"))?;
    result.validate_against_request(request, request_hash).map_err(|e| format!("the worker result does not bind the request: {e}"))?;
    Ok((result, request_hash))
}

// ---------------------------------------------------------------------------------------------
// HTTP plumbing (hand-rolled, one exact surface)
// ---------------------------------------------------------------------------------------------

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).map_err(|e| format!("cannot read the request line: {e}"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| format!("cannot read a header: {e}"))?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().map_err(|_| "content-length is not a number".to_string())?;
        }
    }
    const MAX_BODY: usize = 1 << 20; // 1 MiB of chat is already ~30× the prefill cap
    if content_length > MAX_BODY {
        return Err(format!("body of {content_length} bytes exceeds the {MAX_BODY} cap"));
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).map_err(|e| format!("cannot read the body: {e}"))?;
    Ok(HttpRequest { method, path, body })
}

fn respond(stream: &mut TcpStream, status: &str, body: &serde_json::Value) {
    let bytes = body.to_string().into_bytes();
    let head =
        format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", bytes.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

fn error_body(message: &str) -> serde_json::Value {
    serde_json::json!({ "error": { "message": message, "type": "invalid_request_error" } })
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

// ---------------------------------------------------------------------------------------------
// The one route
// ---------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_chat(config: &Config, identity: &Identity, worker_id: &WorkerIdentity, body: &[u8]) -> Result<serde_json::Value, String> {
    let chat: ChatRequest = serde_json::from_slice(body).map_err(|e| format!("request body is not a chat completion: {e}"))?;
    if chat.stream == Some(true) {
        return Err("streaming is not supported at v1 — the commitment only exists at completion".into());
    }
    let rendered_prompt = render_template_v1(&chat.messages)?;
    let decode_limit = chat.max_tokens.unwrap_or(config.max_decode_default).clamp(1, config.max_decode_cap);

    let (anchor_block, anchor_daa) = load_anchor(&config.anchor_path)?;
    let mut job_nonce = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut job_nonce);

    let request = PalwFpWorkerRequestV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: identity.network_domain,
        class_id: identity.class_id,
        executor_bond: identity.executor_bond,
        executor_pubkey: identity.executor_pubkey.clone(),
        operator_id: identity.operator_id,
        anchor_block,
        anchor_daa,
        job_nonce,
        decode_token_limit: decode_limit,
        max_context_tokens: worker_id.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        input: PalwFpWorkerInputV3::Text(rendered_prompt.clone().into_bytes()),
        model_profile_id: worker_id.model_profile_id,
        runtime_manifest_hash: worker_id.runtime_manifest_hash,
        runtime_class_id: worker_id.runtime_class_id,
        shape_profile_id: worker_id.shape_profile_id,
        trace_scheme_id: worker_id.trace_scheme_id,
    };

    let trace_dir = config.outbox.join("traces");
    std::fs::create_dir_all(&trace_dir).map_err(|e| format!("cannot create the trace retention dir: {e}"))?;
    let (result, _request_hash) = run_worker_v3(&config.worker, &trace_dir, &request)?;
    let job_id = fp_job_id_v3(&result.job);
    let commitment = result.to_commitment(&config.cu_weights, anchor_daa.saturating_add(config.trace_retention_window_daa));
    let cu = commitment.cu;
    let claim_id = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&commitment);
    let quanta = if config.quantum_cu == 0 { 0 } else { fp_quanta_v3(cu, config.quantum_cu, u32::MAX) };

    // The outbox artifact: the framed result (borsh) + a JSON summary. Everything the executor
    // rail needs to assemble, sign and submit the commitment — and an honest list of what is
    // still pending (see the module doc).
    let artifact_stem = format!("fp-job-{}", &hex(job_id)[..16]);
    let artifact_borsh = config.outbox.join(format!("{artifact_stem}.result.borsh"));
    let artifact_json = config.outbox.join(format!("{artifact_stem}.json"));
    let result_bytes = borsh::to_vec(&result).map_err(|e| format!("cannot serialize the artifact: {e}"))?;
    std::fs::write(&artifact_borsh, &result_bytes).map_err(|e| format!("cannot write {}: {e}", artifact_borsh.display()))?;
    let commitment_borsh = config.outbox.join(format!("{artifact_stem}.commitment-unsigned.borsh"));
    let commitment_bytes = borsh::to_vec(&commitment).map_err(|e| format!("cannot serialize the commitment: {e}"))?;
    std::fs::write(&commitment_borsh, &commitment_bytes).map_err(|e| format!("cannot write {}: {e}", commitment_borsh.display()))?;
    let rendered_string = String::from_utf8_lossy(&result.rendered).into_owned();
    let summary = serde_json::json!({
        "schema": "misaka.palw.fp-v3-gateway-artifact.v1",
        "fp_job_id": hex(job_id),
        "template_id": TEMPLATE_ID_V1,
        "prompt_tokens": result.job.prompt_tokens,
        "decode_tokens_executed": result.decode_tokens_executed,
        "decode_token_limit": result.job.decode_token_limit,
        "stop_reason": match result.stop_reason { PalwFpStopReasonV3::ExactBudgetReached => "exact_budget", PalwFpStopReasonV3::EndOfGeneration => "end_of_generation" },
        "fp_claim_id": hex(claim_id),
        "trace_root": hex(result.trace_root),
        "output_root": hex(result.output_root),
        "schedule_root": hex(result.schedule_root),
        "trace_manifest_root": hex(result.trace_manifest_root),
        "trace_chunk_count": result.trace_chunk_count,
        "trace_retention_daa": commitment.trace_retention_daa,
        "trace_dir": trace_dir.join(hex(job_id)).display().to_string(),
        "cu": cu.to_string(),
        "cu_weights": { "prefill": config.cu_weights.prefill_weight, "decode": config.cu_weights.decode_weight },
        "quanta_at_configured_quantum": quanta,
        "answer_untrimmed": rendered_string,
        "pending_for_chain_submission": [
            "ML-DSA-87 signature over fp_claim_id (signer sidecar)",
            "commitment transaction assembly + submission (executor rail, FP-08)",
        ],
    });
    std::fs::write(&artifact_json, serde_json::to_vec_pretty(&summary).unwrap())
        .map_err(|e| format!("cannot write {}: {e}", artifact_json.display()))?;

    let shown = display_trim(&rendered_string).to_string();
    let finish_reason = match result.stop_reason {
        PalwFpStopReasonV3::EndOfGeneration => "stop",
        PalwFpStopReasonV3::ExactBudgetReached => {
            if shown.len() < rendered_string.trim_end().len() {
                "stop" // the guard ended the shown answer; the budget ended the run
            } else {
                "length"
            }
        }
    };
    Ok(serde_json::json!({
        "id": format!("palwcmpl-{}", &hex(job_id)[..24]),
        "object": "chat.completion",
        "model": chat.model.unwrap_or_else(|| "misaka-palw-fp-v3".to_string()),
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": shown },
            "finish_reason": finish_reason,
        }],
        "usage": {
            "prompt_tokens": result.job.prompt_tokens,
            "completion_tokens": result.decode_tokens_executed,
            "total_tokens": result.job.prompt_tokens + result.decode_tokens_executed,
        },
        "misaka": {
            "fp_job_id": hex(job_id),
            "trace_root": hex(result.trace_root),
            "output_root": hex(result.output_root),
            "schedule_root": hex(result.schedule_root),
            "cu": cu.to_string(),
            "artifact": artifact_json.display().to_string(),
        },
    }))
}

fn main() {
    let mut args: VecDeque<String> = std::env::args().skip(1).collect();
    let mut listen = "127.0.0.1:8790".to_string();
    let mut worker: Option<PathBuf> = None;
    let mut outbox: Option<PathBuf> = None;
    let mut identity_path: Option<PathBuf> = None;
    let mut anchor_path: Option<PathBuf> = None;
    let mut cu_prefill: u32 = 1;
    let mut cu_decode: u32 = 64;
    let mut quantum_cu: u128 = 0;
    let mut max_decode_default: u32 = 256;
    let mut max_decode_cap: u32 = 1024;
    let mut trace_retention_window_daa: u64 = 500_000;
    while let Some(arg) = args.pop_front() {
        let mut value = |what: &str| args.pop_front().unwrap_or_else(|| die(format!("{what} needs a value")));
        match arg.as_str() {
            "--listen" => listen = value("--listen"),
            "--worker" => worker = Some(PathBuf::from(value("--worker"))),
            "--outbox" => outbox = Some(PathBuf::from(value("--outbox"))),
            "--identity" => identity_path = Some(PathBuf::from(value("--identity"))),
            "--anchor" => anchor_path = Some(PathBuf::from(value("--anchor"))),
            "--cu-prefill-weight" => cu_prefill = value("--cu-prefill-weight").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--cu-decode-weight" => cu_decode = value("--cu-decode-weight").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--quantum-cu" => quantum_cu = value("--quantum-cu").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--max-decode-default" => {
                max_decode_default = value("--max-decode-default").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--max-decode-cap" => max_decode_cap = value("--max-decode-cap").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--trace-retention-window" => {
                trace_retention_window_daa = value("--trace-retention-window").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-gateway --worker <palw-worker> --outbox <dir> --identity <json> --anchor <json> [--listen addr] [--cu-prefill-weight n] [--cu-decode-weight n] [--quantum-cu n] [--max-decode-default n] [--max-decode-cap n]"
            )),
        }
    }
    let config = Config {
        listen,
        worker: worker.unwrap_or_else(|| die("--worker <palw-worker> is required".into())),
        outbox: outbox.unwrap_or_else(|| die("--outbox <dir> is required".into())),
        identity_path: identity_path.unwrap_or_else(|| die("--identity <json> is required".into())),
        anchor_path: anchor_path.unwrap_or_else(|| die("--anchor <json> is required".into())),
        cu_weights: PalwFpCuWeightsV3 { prefill_weight: cu_prefill, decode_weight: cu_decode },
        quantum_cu,
        max_decode_default,
        max_decode_cap,
        trace_retention_window_daa,
    };
    if cu_decode == 0 {
        die("--cu-decode-weight 0 prices the reference shape at nothing".into());
    }
    std::fs::create_dir_all(&config.outbox).unwrap_or_else(|e| die(format!("cannot create the outbox: {e}")));
    let identity = load_identity(&config.identity_path);
    load_anchor(&config.anchor_path).unwrap_or_else(|e| die(e));
    let worker_id = query_worker_identity(&config.worker);
    if worker_id.prefill_cap == 0 || worker_id.n_ctx == 0 {
        die("the worker's v3-manifest reports no shape limits".into());
    }
    eprintln!(
        "[misaka-palw-gateway] listening on {} — worker manifest {}…, class {}…, template {TEMPLATE_ID_V1}",
        config.listen,
        &hex(worker_id.runtime_manifest_hash)[..16],
        &hex(identity.class_id)[..16],
    );

    // One job at a time: the worker is a whole-model subprocess, and interleaving two would
    // just thrash the page cache. Requests queue on this lock.
    let job_lock = Mutex::new(());
    let listener = TcpListener::bind(&config.listen).unwrap_or_else(|e| die(format!("cannot bind {}: {e}", config.listen)));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let request = match read_http_request(&mut stream) {
            Ok(r) => r,
            Err(e) => {
                respond(&mut stream, "400 Bad Request", &error_body(&e));
                continue;
            }
        };
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/health") => {
                respond(
                    &mut stream,
                    "200 OK",
                    &serde_json::json!({
                        "status": "ok",
                        "runtime_manifest_hash": hex(worker_id.runtime_manifest_hash),
                        "template_id": TEMPLATE_ID_V1,
                    }),
                );
            }
            ("POST", "/v1/chat/completions") => {
                let _running = job_lock.lock().expect("the job lock is never poisoned");
                match handle_chat(&config, &identity, &worker_id, &request.body) {
                    Ok(body) => respond(&mut stream, "200 OK", &body),
                    Err(e) => respond(&mut stream, "400 Bad Request", &error_body(&e)),
                }
            }
            _ => respond(&mut stream, "404 Not Found", &error_body("this gateway serves POST /v1/chat/completions and GET /health")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into() }
    }

    /// **The template is frozen** (its id names this exact transform). A change here is a new
    /// template id and a new class profile — this golden is the tripwire.
    #[test]
    fn template_v1_is_frozen() {
        let rendered = render_template_v1(&[msg("system", "You are a concise assistant."), msg("user", "What is 2+2?")]).unwrap();
        assert_eq!(rendered, "### System:\nYou are a concise assistant.\n\n### User:\nWhat is 2+2?\n\n### Assistant:\n");

        let multi_turn = render_template_v1(&[msg("user", "hi"), msg("assistant", "hello"), msg("user", "bye")]).unwrap();
        assert_eq!(multi_turn, "### User:\nhi\n\n### Assistant:\nhello\n\n### User:\nbye\n\n### Assistant:\n");

        assert!(render_template_v1(&[msg("system", "s")]).is_err(), "no user message is not a chat");
        assert!(render_template_v1(&[msg("tool", "x"), msg("user", "u")]).is_err(), "unknown roles are refused, not dropped");
    }

    /// **F1's gateway face**: the rendered bytes contain the user's words and the template
    /// markers — and nothing else. No DAA strings, no job metadata, no anchor, no nonce.
    #[test]
    fn the_rendered_prompt_carries_no_mining_metadata() {
        let rendered = render_template_v1(&[msg("user", "explain beacons")]).unwrap();
        assert_eq!(rendered, "### User:\nexplain beacons\n\n### Assistant:\n");
        for forbidden in ["daa", "misaka-job", "anchor", "nonce", "bond"] {
            assert!(!rendered.to_lowercase().contains(forbidden), "{forbidden:?} must never enter the model's input");
        }
    }

    /// The display trim is presentation-only and total: with the guard, at it; without, the
    /// whole trimmed answer.
    #[test]
    fn display_trim_stops_at_the_guard() {
        assert_eq!(display_trim("2+2=4.\n\n### User:\nWhat…"), "2+2=4.");
        assert_eq!(display_trim("a full answer with no guard\n"), "a full answer with no guard");
        assert_eq!(display_trim("### at start is content-free"), "### at start is content-free");
    }

    /// The chat request parser accepts the OpenAI subset and refuses a stream request rather
    /// than silently downgrading it.
    #[test]
    fn chat_request_subset_parses() {
        let parsed: ChatRequest =
            serde_json::from_str(r#"{"model":"x","messages":[{"role":"user","content":"hi"}],"max_tokens":32}"#).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.max_tokens, Some(32));
        assert_eq!(parsed.stream, None);

        let stream: ChatRequest = serde_json::from_str(r#"{"messages":[{"role":"user","content":"hi"}],"stream":true}"#).unwrap();
        assert_eq!(stream.stream, Some(true));
    }
}
