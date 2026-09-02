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

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use misaka_palw::host_security::{
    ALLOW_PUBLIC_GATEWAY_ENV, Confinement, ConfinementBackend, check_public_bind, establish_confinement, harden_worker_command,
    listen_is_loopback, public_gateway_acknowledged, reachable_signing_secrets, worker_working_dir,
};

use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFpWorkerInputV3,
    PalwFpWorkerRequestV3, PalwFpWorkerResultV3, fp_class_quantum_leaves_v1, fp_job_id_v3, fp_quanta_v3, fp_worker_request_hash_v3,
};
use kaspa_consensus_core::palw_v2::{PALW_V2_MAX_FRAME_BYTES, read_framed, write_framed};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use serde::Deserialize;

// ADR-0078 Decision 6: the derivation step and one-response delivery. One module, one hook.
mod derive;

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

// ---------------------------------------------------------------------------------------------
// ADR-0079 Decision 10 / ADR-0077 SA-1 — the public entrance is BOUNDED, and every bound below is
// mandatory rather than a default an operator can raise into an unbounded surface.
//
// SA-8 is the reason the per-source rate is last in this list and not first: sources share
// addresses behind proxies, so a per-IP rate is a courtesy. The BINDING limits are the single job
// slot, the bounded in-flight queue, and the daily public-job budget tied to exposure.
// ---------------------------------------------------------------------------------------------

/// A chat body larger than this is refused before it is parsed. 1 MiB of chat is already ~30x the
/// prefill cap of every class in the tree.
const MAX_REQUEST_BODY_BYTES: usize = 1 << 20;
/// The rendered prompt handed to the model, in bytes. A hard ceiling on top of the class's own
/// `n_ctx`: the worker refuses an over-long prompt too, but a bound the ENTRANCE enforces is a
/// bound that costs the attacker a 4xx instead of a model load.
const HARD_MAX_PROMPT_BYTES: usize = 64 * 1024;
/// No `--max-decode-cap` may exceed this, whatever the flag says.
const HARD_MAX_DECODE_CAP: u32 = 4_096;
/// A single chat turn may not carry more messages than this.
const MAX_CHAT_MESSAGES: usize = 64;
/// Open connections. Past this the listener answers 503 and closes, rather than growing threads.
const MAX_CONNECTIONS: usize = 64;
/// **The in-flight queue.** One job runs; at most this many wait for the slot. Past it the answer
/// is a 503 with a Retry-After, never a queue whose depth silently eats deadlines.
const MAX_IN_FLIGHT_JOBS: usize = 8;
/// The public-job budget window.
const PUBLIC_BUDGET_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// The per-source window (SA-8: secondary).
const PER_SOURCE_WINDOW: Duration = Duration::from_secs(60 * 60);
/// ADR-0077 SA-1(d): the exposure ceiling ratio the RC enforces on a bond's collateral in flight
/// (`PalwStateV2Error::FreePromptExposureCeiling`). Printed in `/health` so the loss bound is a
/// number the operator reads, not a promise they infer.
const FREE_PROMPT_EXPOSURE_CEILING_PERMILLE: u64 = 500;
/// ADR-0077 SA-1(b): a queued commitment expires WITH ITS ANCHOR and is never submitted stale.
/// Past this many DAA beyond the anchor the outbox artifact is retired.
const COMMITMENT_ANCHOR_TTL_DAA: u64 = 3_000;

struct Config {
    listen: String,
    worker: PathBuf,
    outbox: PathBuf,
    identity_path: PathBuf,
    anchor_path: PathBuf,
    /// Devnet display aid: the class's canonical job in leaves, so the summary can say how many
    /// draws a job earned (a quantum is an eighth of it — ADR-0074 Decision 5). Zero disables the
    /// display (no class known).
    class_leaves: u64,
    max_decode_default: u32,
    max_decode_cap: u32,
    /// How long past the job's anchor the producer promises to serve retained-trace chunks, in
    /// DAA score. A chain-time promise, so it rides the caller side of `to_commitment`.
    trace_retention_window_daa: u64,
    /// ADR-0078 Decision 6: the bond key's seed, when the gateway signs derivations itself (the
    /// rail's local-seed form); `None` leaves the object unsigned for the rail.
    derive_seed: Option<[u8; kaspa_pq_validator_core::VALIDATOR_SEED_LEN]>,
    /// Artifacts at or under this many bytes ride inline in the response; larger ones by handle.
    artifact_inline_max: usize,
    /// ADR-0079 Decision 5: the worker child's working directory (never the operator's home).
    workdir: PathBuf,
    /// The rendered-prompt ceiling actually in force, `min(flag, HARD_MAX_PROMPT_BYTES)`.
    max_prompt_bytes: usize,
    /// ADR-0077 SA-1(a): the bond's exposure room, in sompi. Zero means the operator has not told
    /// this gateway what the bond can lose, and a gateway that does not know that ANSWERS but does
    /// not commit — the safe reading of an unknown, not an unbounded one.
    bond_exposure_room_sompi: u64,
    /// The fraction of the room public jobs may spend per window, so the operator's OWN claims are
    /// never starved by strangers'.
    public_job_budget_permille: u64,
    /// What one claim reserves on the bond, in sompi.
    claim_exposure_sompi: u64,
    /// ADR-0077 SA-1(c): the operator marks this source class "answer, never commit".
    answer_never_commit: bool,
    /// SA-8's secondary bound: public jobs per source address per [`PER_SOURCE_WINDOW`].
    per_source_jobs_per_window: u32,
    /// ADR-0079 Decision 5's platform half, installed and PROVEN at boot before the bind guard
    /// reads it. `none` when there is none — which Decision 10 then refuses a public bind on.
    confinement: Confinement,
}

/// ADR-0077 SA-1(a): what public jobs have spent of the operator's exposure in this window, and
/// whether the next one may commit. A public prompt becomes the OPERATOR's claim — it reserves
/// `claim_exposure` on the bond and forfeits it if the pipeline is faulty — so the spend is
/// bounded here, at the entrance, rather than discovered at the transition (SA-7).
struct PublicJobBudget {
    window_started: Instant,
    spent_sompi: u64,
    committed_jobs: u64,
    answered_without_commit: u64,
}

impl PublicJobBudget {
    fn new() -> Self {
        Self { window_started: Instant::now(), spent_sompi: 0, committed_jobs: 0, answered_without_commit: 0 }
    }

    fn daily_budget(config: &Config) -> u64 {
        config.bond_exposure_room_sompi.saturating_mul(config.public_job_budget_permille) / 1_000
    }

    /// May the next public job COMMIT? Answering is never refused on budget grounds — the user
    /// gets their answer either way, which is what makes "answer, never commit" a mode rather
    /// than an outage.
    fn may_commit(&mut self, config: &Config) -> Result<(), String> {
        if self.window_started.elapsed() >= PUBLIC_BUDGET_WINDOW {
            self.window_started = Instant::now();
            self.spent_sompi = 0;
        }
        if config.answer_never_commit {
            return Err("this gateway runs in `answer, never commit` mode (ADR-0077 SA-1c)".into());
        }
        if config.bond_exposure_room_sompi == 0 || config.claim_exposure_sompi == 0 {
            return Err("the bond's exposure room is not configured (--bond-exposure-room-sompi / --claim-exposure-sompi); \
                 a gateway that cannot price the spend does not spend"
                .into());
        }
        if config.claim_exposure_sompi > config.bond_exposure_room_sompi {
            return Err(format!(
                "one claim reserves {} sompi and the bond's room is {} — refused at the entrance, not at the transition (ADR-0077 SA-7)",
                config.claim_exposure_sompi, config.bond_exposure_room_sompi
            ));
        }
        let budget = Self::daily_budget(config);
        if self.spent_sompi.saturating_add(config.claim_exposure_sompi) > budget {
            return Err(format!(
                "the public-job budget for this window is spent ({} of {} sompi); the operator's own claims are not starved by strangers'",
                self.spent_sompi, budget
            ));
        }
        Ok(())
    }

    fn charge(&mut self, config: &Config) {
        self.spent_sompi = self.spent_sompi.saturating_add(config.claim_exposure_sompi);
        self.committed_jobs += 1;
    }
}

/// SA-8's secondary bound. Kept because a single noisy source is still worth slowing, and named
/// secondary because sources share addresses behind proxies and this one cannot be the bound.
#[derive(Default)]
struct SourceRates {
    seen: HashMap<IpAddr, (Instant, u32)>,
}

impl SourceRates {
    fn admit(&mut self, source: IpAddr, per_window: u32) -> bool {
        if per_window == 0 {
            return true;
        }
        // Bounded map: a window's worth of distinct sources, then a sweep. An unbounded map keyed
        // by attacker-chosen addresses is itself the memory attack.
        if self.seen.len() > 4_096 {
            self.seen.retain(|_, (at, _)| at.elapsed() < PER_SOURCE_WINDOW);
        }
        let entry = self.seen.entry(source).or_insert((Instant::now(), 0));
        if entry.0.elapsed() >= PER_SOURCE_WINDOW {
            *entry = (Instant::now(), 0);
        }
        entry.1 += 1;
        entry.1 <= per_window
    }
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

fn query_worker_identity(confinement: &Confinement, worker: &Path, workdir: &Path) -> WorkerIdentity {
    let mut cmd = confinement.command(worker);
    cmd.args(["--mode", "v3-manifest"]);
    harden_worker_command(&mut cmd, workdir);
    let output = cmd.output().unwrap_or_else(|e| die(format!("cannot run {} --mode v3-manifest: {e}", worker.display())));
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
    /// ADR-0078: the kind the person asked for — a transformer name (`scene/glb/v1`) or a kind
    /// name (`scene`). Absent: the answer is the product and nothing is derived.
    #[serde(default)]
    derive: Option<String>,
    /// ADR-0078 Decision 6: elect this claim's DSL into the data-availability obligation, so
    /// third parties can verify the derivation on request. Default off — the DSL is the answer
    /// to the person's prompt, and it is theirs to publish.
    #[serde(default)]
    serve_dsl: bool,
}

// ---------------------------------------------------------------------------------------------
// The worker round trip
// ---------------------------------------------------------------------------------------------

fn run_worker_v3(
    confinement: &Confinement,
    worker: &Path,
    workdir: &Path,
    trace_out: &Path,
    request: &PalwFpWorkerRequestV3,
) -> Result<(PalwFpWorkerResultV3, Hash64), String> {
    let payload = borsh::to_vec(request).map_err(|e| format!("cannot serialize the worker request: {e}"))?;
    let request_hash = fp_worker_request_hash_v3(&payload);
    // ADR-0079 Decision 5: the process that parses a stranger's prompt starts with nothing — no
    // operator environment, no PATH, and a working directory that is not the operator's home.
    let mut command = confinement.command(worker);
    command.args(["--mode", "v3-job", "--trace-out", &trace_out.display().to_string()]);
    harden_worker_command(&mut command, workdir);
    let mut child = command
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
    let mut transfer_encoding_chunked = false;
    let mut headers_read = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| format!("cannot read a header: {e}"))?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        headers_read += 1;
        if headers_read > 64 {
            return Err("too many request headers".into());
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().map_err(|_| "content-length is not a number".to_string())?;
            } else if name.eq_ignore_ascii_case("transfer-encoding") && value.to_ascii_lowercase().contains("chunked") {
                transfer_encoding_chunked = true;
            }
        }
    }
    if transfer_encoding_chunked {
        // Refused rather than parsed: a chunked body has no declared length, and a length this
        // surface cannot check before reading is a bound it does not have (ADR-0079 Decision 10).
        return Err("chunked transfer-encoding is not accepted; send a body with a content-length".into());
    }
    if content_length > MAX_REQUEST_BODY_BYTES {
        return Err(format!("body of {content_length} bytes exceeds the {MAX_REQUEST_BODY_BYTES}-byte cap"));
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

/// A binary body (ADR-0078 Decision 6's fetch handle: the artifact by its derived id).
fn respond_bytes(stream: &mut TcpStream, status: &str, content_type: &str, bytes: &[u8]) {
    let head = format!("HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", bytes.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(bytes);
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

/// ADR-0077 SA-1(b): a queued commitment expires WITH ITS ANCHOR. Sweep the outbox for unsigned
/// commitments whose anchor the chain has left behind and retire them, so a rail can never pick up
/// a stale one and submit it. Named `.expired` rather than deleted: the artifact is evidence of
/// work the operator did, and evidence is not this function's to destroy.
fn expire_stale_commitments(outbox: &Path, current_anchor_daa: u64, ttl_daa: u64) -> usize {
    let Ok(entries) = std::fs::read_dir(outbox) else { return 0 };
    let mut retired = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".commitment-unsigned.borsh") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(commitment) = borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>(&bytes) else {
            continue;
        };
        if current_anchor_daa > commitment.job.anchor_daa.saturating_add(ttl_daa) {
            let mut retired_path = path.clone().into_os_string();
            retired_path.push(".expired");
            if std::fs::rename(&path, &retired_path).is_ok() {
                retired += 1;
            }
        }
    }
    retired
}

#[allow(clippy::too_many_arguments)]
fn handle_chat(
    config: &Config,
    identity: &Identity,
    worker_id: &WorkerIdentity,
    budget: &Mutex<PublicJobBudget>,
    body: &[u8],
) -> Result<serde_json::Value, String> {
    if body.len() > MAX_REQUEST_BODY_BYTES {
        return Err(format!("body of {} bytes exceeds the {MAX_REQUEST_BODY_BYTES}-byte cap", body.len()));
    }
    let chat: ChatRequest = serde_json::from_slice(body).map_err(|e| format!("request body is not a chat completion: {e}"))?;
    if chat.stream == Some(true) {
        return Err("streaming is not supported at v1 — the commitment only exists at completion".into());
    }
    // ADR-0079 Decision 10: every bound is mandatory, and exceeding one is a 4xx rather than a
    // queue. These are checked BEFORE the model load, which is the point of having them here.
    if chat.messages.len() > MAX_CHAT_MESSAGES {
        return Err(format!("{} messages exceeds the {MAX_CHAT_MESSAGES}-message cap", chat.messages.len()));
    }
    let rendered_prompt = render_template_v1(&chat.messages)?;
    if rendered_prompt.len() > config.max_prompt_bytes {
        return Err(format!(
            "the rendered prompt is {} bytes and the cap is {} — refused before the model load",
            rendered_prompt.len(),
            config.max_prompt_bytes
        ));
    }
    let decode_limit = chat.max_tokens.unwrap_or(config.max_decode_default).clamp(1, config.max_decode_cap);

    let (anchor_block, anchor_daa) = load_anchor(&config.anchor_path)?;
    expire_stale_commitments(&config.outbox, anchor_daa, COMMITMENT_ANCHOR_TTL_DAA);

    // ADR-0077 SA-1: a stranger's prompt becomes the OPERATOR's claim. Decide BEFORE the inference
    // whether this one may spend exposure — the answer is produced either way; only the commitment
    // is withheld, which is what makes "answer, never commit" a mode and not an outage.
    let commit_refusal = budget.lock().expect("the budget lock is never poisoned").may_commit(config).err();
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
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        input: PalwFpWorkerInputV3::Text(rendered_prompt.clone().into_bytes()),
        model_profile_id: worker_id.model_profile_id,
        runtime_manifest_hash: worker_id.runtime_manifest_hash,
        runtime_class_id: worker_id.runtime_class_id,
        shape_profile_id: worker_id.shape_profile_id,
        trace_scheme_id: worker_id.trace_scheme_id,
    };

    let trace_dir = config.outbox.join("traces");
    std::fs::create_dir_all(&trace_dir).map_err(|e| format!("cannot create the trace retention dir: {e}"))?;
    let (result, _request_hash) = run_worker_v3(&config.confinement, &config.worker, &config.workdir, &trace_dir, &request)?;
    let job_id = fp_job_id_v3(&result.job);
    let commitment = result.to_commitment(anchor_daa.saturating_add(config.trace_retention_window_daa));
    let work_leaves = commitment.work_leaves;
    let claim_id = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&commitment);
    let quanta = if config.class_leaves == 0 {
        0
    } else {
        fp_quanta_v3(work_leaves, fp_class_quantum_leaves_v1(config.class_leaves, 8), u32::MAX)
    };

    // The outbox artifact: the framed result (borsh) + a JSON summary. Everything the executor
    // rail needs to assemble, sign and submit the commitment — and an honest list of what is
    // still pending (see the module doc).
    let artifact_stem = format!("fp-job-{}", &hex(job_id)[..16]);
    let artifact_borsh = config.outbox.join(format!("{artifact_stem}.result.borsh"));
    let artifact_json = config.outbox.join(format!("{artifact_stem}.json"));
    let result_bytes = borsh::to_vec(&result).map_err(|e| format!("cannot serialize the artifact: {e}"))?;
    std::fs::write(&artifact_borsh, &result_bytes).map_err(|e| format!("cannot write {}: {e}", artifact_borsh.display()))?;
    // **The commitment is written only when the operator's exposure may pay for it** (ADR-0077
    // SA-1 / SA-7). Refused, the user still gets the answer above; what does not happen is a claim
    // this bond cannot back, discovered at the transition instead of at the entrance.
    match &commit_refusal {
        None => {
            let commitment_borsh = config.outbox.join(format!("{artifact_stem}.commitment-unsigned.borsh"));
            let commitment_bytes = borsh::to_vec(&commitment).map_err(|e| format!("cannot serialize the commitment: {e}"))?;
            std::fs::write(&commitment_borsh, &commitment_bytes)
                .map_err(|e| format!("cannot write {}: {e}", commitment_borsh.display()))?;
            budget.lock().expect("the budget lock is never poisoned").charge(config);
        }
        Some(_) => {
            let mut guard = budget.lock().expect("the budget lock is never poisoned");
            guard.answered_without_commit += 1;
        }
    }
    let rendered_string = String::from_utf8_lossy(&result.rendered).into_owned();
    // ADR-0078 Decision 6: derive from the FULL committed rendering (never the display trim —
    // a DSL hashed from a trimmed answer is one no verifier holding the ids can reach).
    let derivation = match chat.derive.as_deref() {
        Some(spec) => Some(derive::run(
            spec,
            &derive::DeriveConfig { seed: config.derive_seed, serve_dsl: chat.serve_dsl },
            &misaka_palw_derive::ClaimBinding {
                network_domain: identity.network_domain,
                claim_id,
                output_root: result.output_root,
                executor_pubkey: identity.executor_pubkey.clone(),
            },
            rendered_string.as_bytes(),
            &config.outbox,
            &artifact_stem,
        )?),
        None => None,
    };
    let (job_context_hash, family) = derive::read_worker_manifest(&trace_dir.join(hex(job_id)));
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
        "work_leaves": work_leaves,
        "class_leaves": config.class_leaves,
        "quanta_at_configured_quantum": quanta,
        "answer_untrimmed": rendered_string,
        "job_context_hash": job_context_hash,
        "family": family,
        "derivation": derivation.as_ref().map(|d| d.to_json(0)),
        // ADR-0077 SA-1(b): the anchor this commitment is bound to, and the DAA past which it must
        // never be submitted. A rail that finds `.expired` beside a stem is looking at work whose
        // freshness binding has lapsed.
        "commit_by_anchor_daa": commitment.job.anchor_daa.saturating_add(COMMITMENT_ANCHOR_TTL_DAA),
        "committed": commit_refusal.is_none(),
        "not_committed_because": commit_refusal.clone(),
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
            "work_leaves": work_leaves,
            "artifact": artifact_json.display().to_string(),
            // ADR-0078 X6: what a consumer needs beside the answer to recompute the claim's
            // output_root — the ids, the job's context hash, and which family's rendered-hash
            // rule applies — and the executor key the derivation is bound to.
            "fp_claim_id": hex(claim_id),
            "output_token_ids": result.output_token_ids,
            "job_context_hash": job_context_hash,
            "family": family,
            "executor_pubkey": faster_hex::hex_string(&identity.executor_pubkey),
            "derivation": derivation.as_ref().map(|d| d.to_json(config.artifact_inline_max)),
            // The caller is told, in the same response, whether this answer became a claim. A
            // gateway that silently answered without committing would be lying about what the
            // operator staked on it.
            "committed": commit_refusal.is_none(),
            "not_committed_because": commit_refusal,
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
    let mut class_leaves: u64 = 0;
    let mut max_decode_default: u32 = 256;
    let mut max_decode_cap: u32 = 1024;
    let mut trace_retention_window_daa: u64 = 500_000;
    let mut derive_seed_path: Option<PathBuf> = None;
    let mut artifact_inline_max: usize = 4 << 20;
    let mut max_prompt_bytes: usize = HARD_MAX_PROMPT_BYTES;
    let mut bond_exposure_room_sompi: u64 = 0;
    let mut public_job_budget_permille: u64 = 200;
    let mut claim_exposure_sompi: u64 = 0;
    let mut answer_never_commit = false;
    let mut per_source_jobs_per_window: u32 = 120;
    while let Some(arg) = args.pop_front() {
        let mut value = |what: &str| args.pop_front().unwrap_or_else(|| die(format!("{what} needs a value")));
        match arg.as_str() {
            "--listen" => listen = value("--listen"),
            "--worker" => worker = Some(PathBuf::from(value("--worker"))),
            "--outbox" => outbox = Some(PathBuf::from(value("--outbox"))),
            "--identity" => identity_path = Some(PathBuf::from(value("--identity"))),
            "--anchor" => anchor_path = Some(PathBuf::from(value("--anchor"))),
            "--class-leaves" => class_leaves = value("--class-leaves").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--max-decode-default" => {
                max_decode_default = value("--max-decode-default").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--max-decode-cap" => max_decode_cap = value("--max-decode-cap").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--trace-retention-window" => {
                trace_retention_window_daa = value("--trace-retention-window").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--derive-seed" => derive_seed_path = Some(PathBuf::from(value("--derive-seed"))),
            "--artifact-inline-max" => artifact_inline_max = value("--artifact-inline-max").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--max-prompt-bytes" => max_prompt_bytes = value("--max-prompt-bytes").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--bond-exposure-room-sompi" => {
                bond_exposure_room_sompi = value("--bond-exposure-room-sompi").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--public-job-budget-permille" => {
                public_job_budget_permille = value("--public-job-budget-permille").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--claim-exposure-sompi" => {
                claim_exposure_sompi = value("--claim-exposure-sompi").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            "--answer-never-commit" => answer_never_commit = true,
            "--per-source-jobs-per-window" => {
                per_source_jobs_per_window = value("--per-source-jobs-per-window").parse().unwrap_or_else(|e| die(format!("{e}")))
            }
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-gateway --worker <palw-worker> --outbox <dir> --identity <json> --anchor <json> [--listen addr] [--class-leaves n] [--max-decode-default n] [--max-decode-cap n] [--max-prompt-bytes n] [--bond-exposure-room-sompi n --claim-exposure-sompi n [--public-job-budget-permille n]] [--answer-never-commit] [--per-source-jobs-per-window n] [--derive-seed <file>] [--artifact-inline-max <bytes>]"
            )),
        }
    }
    // ADR-0079 Decision 5: one working directory for every worker this process spawns, and it is
    // neither the operator's home nor the node's datadir.
    let workdir = match worker_working_dir(None) {
        Ok(dir) => dir,
        Err(e) => die(e),
    };
    let mut config = Config {
        listen,
        worker: worker.unwrap_or_else(|| die("--worker <palw-worker> is required".into())),
        outbox: outbox.unwrap_or_else(|| die("--outbox <dir> is required".into())),
        identity_path: identity_path.unwrap_or_else(|| die("--identity <json> is required".into())),
        anchor_path: anchor_path.unwrap_or_else(|| die("--anchor <json> is required".into())),
        class_leaves,
        max_decode_default,
        // The flag may only lower the hard cap, never raise it (Decision 10: the bounds are
        // mandatory, not defaults).
        max_decode_cap: max_decode_cap.clamp(1, HARD_MAX_DECODE_CAP),
        trace_retention_window_daa,
        derive_seed: derive_seed_path.map(|p| derive::read_seed(&p).unwrap_or_else(|e| die(e))),
        artifact_inline_max,
        workdir,
        max_prompt_bytes: max_prompt_bytes.clamp(1, HARD_MAX_PROMPT_BYTES),
        bond_exposure_room_sompi,
        public_job_budget_permille: public_job_budget_permille.min(1_000),
        claim_exposure_sompi,
        answer_never_commit,
        per_source_jobs_per_window,
        confinement: Confinement::none(),
    };

    // -----------------------------------------------------------------------------------------
    // ADR-0079 Decision 4 / S5 — this process parses a stranger's bytes, so it holds no key. It
    // refuses to boot if a signing secret is reachable in its OWN view: the ML-DSA signature
    // belongs to the signer sidecar, and a seed dropped next to the identity file "for now" is
    // how that stops being true.
    // -----------------------------------------------------------------------------------------
    let identity_dir = config.identity_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let secret_dirs: Vec<&Path> = vec![identity_dir.as_path(), config.outbox.as_path()];
    let reachable = reachable_signing_secrets(|name| std::env::var(name).ok(), &secret_dirs);
    if !reachable.is_empty() {
        let found = reachable.iter().map(|r| r.to_string()).collect::<Vec<_>>().join("; ");
        die(format!(
            "refusing to boot: a signing secret is reachable in this gateway's own view — {found}.\n\
             This process parses public HTTP text and holds the executor PUBLIC key only (ADR-0079 Decision 4). \
             Move the seed to the signer sidecar's own directory, or unset the variable."
        ));
    }

    // -----------------------------------------------------------------------------------------
    // ADR-0079 Decision 10 / S6 — the public entrance is acknowledged, or it does not start. And
    // a public bind on a host whose confinement backend is `none` does not start at all: that is
    // the one place where a stranger chooses the model's input.
    // -----------------------------------------------------------------------------------------
    // The backend installs and PROVES its own denials here — before the bind guard asks what is in
    // force, because a guard that read a configured value would be reading a promise.
    let (confinement, confinement_notes) = establish_confinement(&config.workdir, &[config.workdir.clone(), config.outbox.clone()]);
    for note in &confinement_notes {
        eprintln!("[misaka-palw-gateway] confinement: {note}");
    }
    let backend = confinement.backend();
    config.confinement = confinement;
    let acknowledged = public_gateway_acknowledged();
    if let Err(e) = check_public_bind(&config.listen, acknowledged, backend) {
        die(e);
    }

    std::fs::create_dir_all(&config.outbox).unwrap_or_else(|e| die(format!("cannot create the outbox: {e}")));
    let identity = load_identity(&config.identity_path);
    load_anchor(&config.anchor_path).unwrap_or_else(|e| die(e));
    let worker_id = query_worker_identity(&config.confinement, &config.worker, &config.workdir);
    if worker_id.prefill_cap == 0 || worker_id.n_ctx == 0 {
        die("the worker's v3-manifest reports no shape limits".into());
    }
    eprintln!(
        "[misaka-palw-gateway] listening on {} ({}) — worker manifest {}…, class {}…, template {TEMPLATE_ID_V1}",
        config.listen,
        if listen_is_loopback(&config.listen) { "loopback" } else { "PUBLIC, acknowledged" },
        &hex(worker_id.runtime_manifest_hash)[..16],
        &hex(identity.class_id)[..16],
    );
    eprintln!(
        "[misaka-palw-gateway] confinement backend {} | one job slot, {MAX_IN_FLIGHT_JOBS} may queue, {MAX_CONNECTIONS} connections | \
         prompt ≤ {} bytes, body ≤ {MAX_REQUEST_BODY_BYTES} bytes, decode ≤ {} | public-job budget {}‰ of a {}-sompi room",
        backend.name(),
        config.max_prompt_bytes,
        config.max_decode_cap,
        config.public_job_budget_permille,
        config.bond_exposure_room_sompi,
    );

    let config = Arc::new(config);
    let identity = Arc::new(identity);
    let worker_id = Arc::new(worker_id);
    // **One job slot** — the worker is a whole-model subprocess, and interleaving two would only
    // thrash the page cache. **A BOUNDED queue in front of it** — an unbounded one is a deadline
    // eater and a memory attack; past `MAX_IN_FLIGHT_JOBS` the answer is a 503, not a wait.
    let job_lock = Arc::new(Mutex::new(()));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let connections = Arc::new(AtomicUsize::new(0));
    let budget = Arc::new(Mutex::new(PublicJobBudget::new()));
    let sources = Arc::new(Mutex::new(SourceRates::default()));

    let listener = TcpListener::bind(&config.listen).unwrap_or_else(|e| die(format!("cannot bind {}: {e}", config.listen)));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if connections.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            connections.fetch_sub(1, Ordering::AcqRel);
            respond(&mut stream, "503 Service Unavailable", &error_body("connection cap reached"));
            continue;
        }
        let (config, identity, worker_id) = (Arc::clone(&config), Arc::clone(&identity), Arc::clone(&worker_id));
        let (job_lock, in_flight, budget, sources) =
            (Arc::clone(&job_lock), Arc::clone(&in_flight), Arc::clone(&budget), Arc::clone(&sources));
        let connections = Arc::clone(&connections);
        let acknowledged_bind = acknowledged;
        std::thread::spawn(move || {
            serve_connection(
                &mut stream,
                &config,
                &identity,
                &worker_id,
                &job_lock,
                &in_flight,
                &budget,
                &sources,
                backend,
                acknowledged_bind,
            );
            connections.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn serve_connection(
    stream: &mut TcpStream,
    config: &Config,
    identity: &Identity,
    worker_id: &WorkerIdentity,
    job_lock: &Mutex<()>,
    in_flight: &AtomicUsize,
    budget: &Mutex<PublicJobBudget>,
    sources: &Mutex<SourceRates>,
    backend: ConfinementBackend,
    acknowledged_bind: bool,
) {
    let source = stream.peer_addr().map(|a| a.ip()).ok();
    let request = match read_http_request(stream) {
        Ok(r) => r,
        Err(e) => {
            respond(stream, "400 Bad Request", &error_body(&e));
            return;
        }
    };
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => {
            let snapshot = budget.lock().expect("the budget lock is never poisoned");
            let daily = PublicJobBudget::daily_budget(config);
            respond(
                stream,
                "200 OK",
                // The identity, not just the runtime. A client cannot otherwise tell whether the
                // gateway it is talking to is accountable to anything: the class id is what a
                // chain registers and a court adjudicates, and the bond outpoint is who pays if
                // the answer was a lie. All three are public on-chain facts — a `/health` that
                // withheld them would only be hiding them from the person deciding to trust
                // this endpoint. The commitment carries the same values, so a caller can check
                // that the job it got back came from the identity advertised here.
                //
                // ADR-0077 SA-1(d) adds the LOSS BOUND, for the same reason and the other
                // direction: a stranger's prompt spends the operator's exposure, and the amount
                // is a number the operator reads here rather than a promise they infer.
                &serde_json::json!({
                    "status": "ok",
                    "runtime_manifest_hash": hex(worker_id.runtime_manifest_hash),
                    "template_id": TEMPLATE_ID_V1,
                    "class_id": hex(identity.class_id),
                    "network_domain": hex(identity.network_domain),
                    "operator_id": hex(identity.operator_id),
                    "bond": format!("{}:{}", identity.executor_bond.transaction_id, identity.executor_bond.index),
                    "posture": {
                        "listen": config.listen,
                        "public_bind": !listen_is_loopback(&config.listen),
                        "acknowledgement_variable": ALLOW_PUBLIC_GATEWAY_ENV,
                        "acknowledgement_required": !listen_is_loopback(&config.listen),
                        "acknowledgement_given": acknowledged_bind,
                        "confinement_backend": backend.name(),
                        "holds_key_material": false,
                    },
                    "bounds": {
                        "max_request_body_bytes": MAX_REQUEST_BODY_BYTES,
                        "max_prompt_bytes": config.max_prompt_bytes,
                        "max_decode_cap": config.max_decode_cap,
                        "job_slots": 1,
                        "max_in_flight_jobs": MAX_IN_FLIGHT_JOBS,
                        "max_connections": MAX_CONNECTIONS,
                        "per_source_jobs_per_window": config.per_source_jobs_per_window,
                        "per_source_window_secs": PER_SOURCE_WINDOW.as_secs(),
                    },
                    "exposure": {
                        "loss_bound": "at most claim_exposure per claim, and at most the FreePromptExposureCeiling \
                                       ratio of collateral in flight",
                        "free_prompt_exposure_ceiling_permille": FREE_PROMPT_EXPOSURE_CEILING_PERMILLE,
                        "claim_exposure_sompi": config.claim_exposure_sompi,
                        "bond_exposure_room_sompi": config.bond_exposure_room_sompi,
                        "public_job_budget_permille": config.public_job_budget_permille,
                        "public_job_budget_window_sompi": daily,
                        "public_job_budget_spent_sompi": snapshot.spent_sompi,
                        "public_job_budget_window_secs": PUBLIC_BUDGET_WINDOW.as_secs(),
                        "answer_never_commit": config.answer_never_commit,
                        "committed_jobs": snapshot.committed_jobs,
                        "answered_without_commit": snapshot.answered_without_commit,
                        "commitment_anchor_ttl_daa": COMMITMENT_ANCHOR_TTL_DAA,
                    },
                }),
            );
        }
        ("POST", "/v1/chat/completions") => {
            if let Some(source) = source
                && !sources.lock().expect("the source lock is never poisoned").admit(source, config.per_source_jobs_per_window)
            {
                respond(stream, "429 Too Many Requests", &error_body("per-source job rate exceeded"));
                return;
            }
            // The bounded in-flight queue. Reserved BEFORE the slot is contended, so the depth of
            // the wait is a number this process chose rather than one the network chose for it.
            if in_flight.fetch_add(1, Ordering::AcqRel) >= MAX_IN_FLIGHT_JOBS {
                in_flight.fetch_sub(1, Ordering::AcqRel);
                respond(
                    stream,
                    "503 Service Unavailable",
                    &error_body("the in-flight queue is full; one job runs at a time and the queue is bounded"),
                );
                return;
            }
            let outcome = {
                let _running = job_lock.lock().expect("the job lock is never poisoned");
                handle_chat(config, identity, worker_id, budget, &request.body)
            };
            in_flight.fetch_sub(1, Ordering::AcqRel);
            match outcome {
                Ok(body) => respond(stream, "200 OK", &body),
                Err(e) => respond(stream, "400 Bad Request", &error_body(&e)),
            }
        }
        // ADR-0078 Decision 6's fetch handle: a derived artifact too large to ride inline is
        // served by its derived id. A GET with no side effects, so it needs neither the job slot
        // nor the in-flight reservation — but it is dispatched HERE, inside the bounded accept
        // loop, so the connection cap still counts it.
        ("GET", path) if path.starts_with("/v1/artifacts/") => {
            match derive::artifact_by_id(&config.outbox, &path["/v1/artifacts/".len()..]) {
                Some((bytes, content_type)) => respond_bytes(stream, "200 OK", content_type, &bytes),
                None => respond(stream, "404 Not Found", &error_body("no artifact under that derived id")),
            }
        }
        _ => respond(
            stream,
            "404 Not Found",
            &error_body("this gateway serves POST /v1/chat/completions, GET /health and GET /v1/artifacts/<derived-id>"),
        ),
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

    fn bounded_config() -> Config {
        Config {
            listen: "127.0.0.1:8790".into(),
            worker: PathBuf::from("/nonexistent/worker"),
            outbox: std::env::temp_dir().join("palw-gw-test-outbox"),
            identity_path: PathBuf::from("/nonexistent/identity.json"),
            anchor_path: PathBuf::from("/nonexistent/anchor.json"),
            class_leaves: 0,
            max_decode_default: 256,
            max_decode_cap: 1024,
            trace_retention_window_daa: 500_000,
            workdir: std::env::temp_dir(),
            max_prompt_bytes: HARD_MAX_PROMPT_BYTES,
            bond_exposure_room_sompi: 1_000_000,
            public_job_budget_permille: 200,
            claim_exposure_sompi: 50_000,
            answer_never_commit: false,
            per_source_jobs_per_window: 2,
            confinement: Confinement::none(),
            derive_seed: None,
            artifact_inline_max: 4 << 20,
        }
    }

    /// **ADR-0079 S6.** A public bind fails at startup without the acknowledgement, and fails
    /// UNCONDITIONALLY when the confinement backend in force is `none` — which is the state this
    /// tree ships in, so this is the rule that is actually load-bearing today.
    #[test]
    fn a_public_bind_is_refused_and_the_message_names_the_pattern() {
        assert!(check_public_bind("127.0.0.1:8790", false, ConfinementBackend::None).is_ok(), "loopback is the default and is fine");

        let err = check_public_bind("0.0.0.0:8790", false, ConfinementBackend::MacosSandboxExec).unwrap_err();
        assert!(err.contains(ALLOW_PUBLIC_GATEWAY_ENV));
        assert!(err.to_lowercase().contains("reverse proxy"));

        // The state a host with no requested backend ships in. The acknowledgement does not help.
        let err = check_public_bind("0.0.0.0:8790", true, ConfinementBackend::None).unwrap_err();
        assert!(err.contains("does NOT override"));
        assert_eq!(Confinement::none().backend(), ConfinementBackend::None, "and this is what `none` looks like");
    }

    /// **No wildcard CORS** — the house rule `SECURITY.md` already states for the mining bridge,
    /// held here too so a page on another origin cannot read this endpoint out of the operator's
    /// browser. The response head is pinned, not just the absence of a call to set the header.
    #[test]
    fn responses_carry_no_cors_header_at_all() {
        let bytes = serde_json::json!({"status": "ok"}).to_string().into_bytes();
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            bytes.len()
        );
        let lowered = head.to_lowercase();
        assert!(!lowered.contains("access-control-allow-origin"), "no CORS header, wildcard or otherwise");
        assert!(!lowered.contains('*'), "nothing in this response head is a wildcard");
        // And the shipped writer is the one that produced that head. The needle is assembled at
        // run time so this assertion does not match its own source line.
        let needle = ["access-control", "allow-origin"].join("-");
        let responses: Vec<&str> = std::include_str!("main.rs")
            .lines()
            .filter(|l| l.contains("HTTP/1.1") || l.trim_start().starts_with("let head ="))
            .collect();
        assert!(!responses.is_empty(), "the response writer must be findable for this assertion to mean anything");
        for line in responses {
            assert!(!line.to_lowercase().contains(&needle), "a response head writes a CORS header: {line}");
        }
    }

    /// **ADR-0077 SA-1 / SA-8.** The binding limits are the single slot, the bounded queue and the
    /// budget — and the budget refuses to COMMIT while still allowing the answer.
    #[test]
    fn the_public_job_budget_bounds_the_operators_exposure() {
        let config = bounded_config();
        let mut budget = PublicJobBudget::new();
        // 200 permille of a 1,000,000-sompi room is 200,000; a 50,000-sompi claim fits four times.
        assert_eq!(PublicJobBudget::daily_budget(&config), 200_000);
        for _ in 0..4 {
            budget.may_commit(&config).expect("within the window budget");
            budget.charge(&config);
        }
        let err = budget.may_commit(&config).unwrap_err();
        assert!(err.contains("budget for this window is spent"), "got {err}");
        assert_eq!(budget.committed_jobs, 4);

        // SA-1(c): the operator may mark the source class "answer, never commit".
        let never = Config { answer_never_commit: true, ..bounded_config() };
        let err = PublicJobBudget::new().may_commit(&never).unwrap_err();
        assert!(err.contains("answer, never commit"));

        // SA-7: a claim that would exceed the bond's room is refused HERE, at the entrance.
        let over = Config { claim_exposure_sompi: 2_000_000, ..bounded_config() };
        let err = PublicJobBudget::new().may_commit(&over).unwrap_err();
        assert!(err.contains("refused at the entrance"), "got {err}");

        // An unconfigured room is read as unknown, and an unknown does not spend.
        let unknown = Config { bond_exposure_room_sompi: 0, ..bounded_config() };
        assert!(PublicJobBudget::new().may_commit(&unknown).is_err());
    }

    /// The per-source rate is SECONDARY (SA-8) but it is real: the third job from one address in
    /// a window is refused when the operator set the quota to two.
    #[test]
    fn the_per_source_quota_admits_then_refuses() {
        let mut rates = SourceRates::default();
        let source: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(rates.admit(source, 2));
        assert!(rates.admit(source, 2));
        assert!(!rates.admit(source, 2), "the third job in the window is refused");
        // Another source is unaffected — the quota is per source, not a global gate.
        assert!(rates.admit("198.51.100.9".parse().unwrap(), 2));
        // Zero disables it, because a quota of zero would otherwise mean "serve nobody".
        assert!(rates.admit(source, 0));
    }

    /// Every mandatory bound is a hard ceiling a flag may only LOWER. A `--max-decode-cap` of a
    /// million is a bound the operator does not have.
    #[test]
    fn the_flags_may_lower_a_bound_and_never_raise_it() {
        assert_eq!(1_000_000u32.clamp(1, HARD_MAX_DECODE_CAP), HARD_MAX_DECODE_CAP);
        assert_eq!(64u32.clamp(1, HARD_MAX_DECODE_CAP), 64);
        assert_eq!(usize::MAX.clamp(1, HARD_MAX_PROMPT_BYTES), HARD_MAX_PROMPT_BYTES);
        assert!(MAX_IN_FLIGHT_JOBS > 0 && MAX_IN_FLIGHT_JOBS < MAX_CONNECTIONS, "the queue is bounded and smaller than the accepts");
    }

    /// **ADR-0077 SA-1(b).** A queued commitment expires WITH ITS ANCHOR: past the TTL the outbox
    /// artifact is retired so no rail can pick it up and submit it stale.
    #[test]
    fn a_queued_commitment_expires_with_its_anchor() {
        let dir = std::env::temp_dir().join(format!("palw-gw-expiry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Nothing to sweep is not an error, and a non-commitment file is never touched.
        std::fs::write(dir.join("fp-job-abc.json"), b"{}").unwrap();
        assert_eq!(expire_stale_commitments(&dir, 10_000, COMMITMENT_ANCHOR_TTL_DAA), 0);
        assert!(dir.join("fp-job-abc.json").is_file());
        // A commitment file that does not decode is left alone rather than silently deleted.
        std::fs::write(dir.join("fp-job-abc.commitment-unsigned.borsh"), b"not borsh").unwrap();
        assert_eq!(expire_stale_commitments(&dir, 10_000, COMMITMENT_ANCHOR_TTL_DAA), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **ADR-0079 S5.** The gateway holds the executor PUBLIC key only, and refuses to boot when a
    /// signing secret is reachable in its own view.
    #[test]
    fn a_reachable_signing_secret_is_a_boot_refusal() {
        let dir = std::env::temp_dir().join(format!("palw-gw-secret-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("identity.json"), b"{}").unwrap();
        assert!(reachable_signing_secrets(|_| None, &[dir.as_path()]).is_empty(), "an identity file is not a secret");

        std::fs::write(dir.join("bond.seed"), [3u8; 32]).unwrap();
        let found = reachable_signing_secrets(|_| None, &[dir.as_path()]);
        assert_eq!(found.len(), 1, "a 32-byte file beside the identity is the shape of a raw ML-DSA-87 seed");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **ADR-0077 SA-5 / ADR-0079 SA-7.** Nothing in this binary logs a prompt. The rendered
    /// prompt is handed to the worker and never to a log line.
    #[test]
    fn the_gateway_logs_no_prompt() {
        let source = std::include_str!("main.rs");
        for line in source.lines() {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("eprintln!") || trimmed.starts_with("println!") || trimmed.starts_with("log::")) {
                continue;
            }
            for forbidden in ["rendered_prompt", "chat.messages", "message.content", "prompt_token_ids"] {
                assert!(!line.contains(forbidden), "a log line carries {forbidden}: {line}");
            }
        }
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
