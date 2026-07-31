//! The bridge's HTTP surface — a hand-written HTTP/1.1 server over raw tokio, the same house
//! pattern as `mtp/service/src/http.rs` and `rpc/eth` (the workspace pins tokio 1.42.1, which
//! rules out axum/hyper): `TcpListener` + a connection semaphore + a whole-connection timeout +
//! head/body caps + `Connection: close`.
//!
//! Routes (the palw-gateway coordinator protocol v1, served under `/palw/v1`):
//! * `POST /palw/v1/jobs`                          → `{accepted:true}` (idempotent by job_id)
//! * `POST /palw/v1/verdicts` `{job_ids:[…]}`      → `{verdicts:[{job_id,verdict}]}`
//! * `GET  /palw/v1/assignments?provider_id=X`     → `{assignments:[…]}` (claim-on-fetch)
//! * `POST /palw/v1/assignments/{job}/decline`     → `{declined:true}`
//! * `POST /palw/v1/replica-results`               → `{recorded:true, matched:bool}`
//! * `GET  /palw/v1/status`                        → journal head/seq, job phases, providers
//! * `GET  /health`                                → liveness (always unauthenticated)
//!
//! With `--auth-token`, every `/palw/v1/*` request must carry `Authorization: Bearer <token>`.
//!
//! The state mutex is a sync `std::sync::Mutex` held across the (fsync-ed) journal append —
//! deliberate: appends are small and rare relative to inference timescales, and a total order
//! over events is exactly what the hash chain wants. Not a throughput surface.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

use crate::chain::ChainFacts;
use crate::da::DaResponseWire;
use crate::provider::ProviderRegistrationV1;
use crate::state::BridgeState;
use crate::wire::{JobSubmissionV1, ReplicaResultV1, RuntimeRootsV1};

const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Body cap: prompt_ids for a 32k-token context serialize to well under 1 MiB; 16 MiB leaves
/// room without inviting abuse.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 128;
const CONN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct HttpConfig {
    pub listen: SocketAddr,
    pub auth_token: Option<String>,
    /// The chain-facts source. `None` ⇒ consensus seams are OFF (dev-harness mode).
    pub chain: Option<Arc<dyn ChainFacts>>,
    /// Require bonded, signed providers on every consequential route.
    pub require_bonded: bool,
    pub network_id: u32,
}

pub async fn serve(state: Arc<Mutex<BridgeState>>, config: HttpConfig) -> Result<(), String> {
    let listener = TcpListener::bind(config.listen).await.map_err(|e| format!("bind {}: {e}", config.listen))?;
    eprintln!("[palw-bridge] listening on http://{}", config.listen);
    let semaphore = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let config = Arc::new(config);
    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("[palw-bridge] accept error: {e}");
                continue;
            }
        };
        let Ok(permit) = Arc::clone(&semaphore).try_acquire_owned() else {
            continue; // over capacity: drop, don't queue (backpressure)
        };
        let state = Arc::clone(&state);
        let config = Arc::clone(&config);
        tokio::spawn(async move {
            let _permit = permit;
            let _ = tokio::time::timeout(CONN_TIMEOUT, handle_connection(stream, state, config)).await;
        });
    }
}

struct ParsedRequest {
    method: String,
    path: String,
    query: String,
    authorization: Option<String>,
    /// `X-Palw-Signature`: the provider's ML-DSA-87 signature over this route + body.
    provider_signature: Option<String>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> Result<ParsedRequest, String> {
    let mut buf = Vec::with_capacity(4096);
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err("request head too large".into());
        }
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await.map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("connection closed before request head".into());
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let head = std::str::from_utf8(&buf[..head_end]).map_err(|_| "non-utf8 head".to_string())?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or("empty request")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("bad request line")?.to_uppercase();
    let target = parts.next().ok_or("bad request line")?;
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };

    let mut content_length = 0usize;
    let mut authorization = None;
    let mut provider_signature = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().map_err(|_| "bad content-length".to_string())?;
        } else if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("x-palw-signature") {
            provider_signature = Some(value.to_string());
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err("body too large".into());
    }

    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_length {
        let mut chunk = [0u8; 8192];
        let n = stream.read(&mut chunk).await.map_err(|e| format!("read body: {e}"))?;
        if n == 0 {
            return Err("connection closed mid-body".into());
        }
        body.extend_from_slice(&chunk[..n]);
        if body.len() > MAX_BODY_BYTES {
            return Err("body too large".into());
        }
    }
    body.truncate(content_length);
    Ok(ParsedRequest { method, path, query, authorization, provider_signature, body })
}

async fn write_response(stream: &mut TcpStream, code: u16, body: &Value) {
    let reason = match code {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(&bytes).await;
    let _ = stream.flush().await;
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| v.to_string())
    })
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

async fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<BridgeState>>, config: Arc<HttpConfig>) {
    let request = match read_request(&mut stream).await {
        Ok(r) => r,
        Err(e) => {
            write_response(&mut stream, 400, &json!({ "error": { "message": e } })).await;
            return;
        }
    };

    if request.path == "/health" {
        write_response(&mut stream, 200, &json!({ "ok": true })).await;
        return;
    }
    if let Some(expected) = &config.auth_token {
        let ok = request.authorization.as_deref() == Some(&format!("Bearer {expected}"));
        if !ok {
            write_response(&mut stream, 401, &json!({ "error": { "message": "missing or wrong bearer token" } })).await;
            return;
        }
    }

    let (code, body) = dispatch(&request, &state, &config);
    write_response(&mut stream, code, &body).await;
}

/// Provider authentication for the routes that need it. Returns the authenticated bond
/// outpoint. In dev-harness mode (no chain facts, `--require-bonded` off) this is skipped and
/// the caller falls back to the self-declared id — which is exactly why `/palw/v1/status`
/// reports the mode.
fn authenticate(
    request: &ParsedRequest,
    state: &Mutex<BridgeState>,
    config: &HttpConfig,
    declared_bond: &str,
) -> Result<Option<String>, String> {
    let Some(chain) = &config.chain else {
        if config.require_bonded {
            return Err("bridge requires bonded providers but has no chain-facts source".into());
        }
        return Ok(None);
    };
    if !config.require_bonded {
        return Ok(None);
    }
    let signature = request
        .provider_signature
        .as_deref()
        .ok_or("missing X-Palw-Signature (this bridge requires bonded, signed providers)")?;
    let guard = state.lock().unwrap();
    let provider = guard.authenticate(declared_bond, &request.path, &request.body, signature, chain.as_ref())?;
    Ok(Some(provider.bond_outpoint.clone()))
}

fn dispatch(request: &ParsedRequest, state: &Mutex<BridgeState>, config: &HttpConfig) -> (u16, Value) {
    let now = now_unix_ms();
    let chain = config.chain.clone();
    let parse_body = || -> Result<Value, String> {
        if request.body.is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_slice(&request.body).map_err(|e| format!("bad json: {e}"))
    };
    let outcome: Result<Value, String> = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/palw/v1/status") => {
            let guard = state.lock().unwrap();
            let mut status = guard.status_json();
            let seams = match &chain {
                Some(facts) => serde_json::json!({
                    "enabled": config.require_bonded,
                    "chain_facts": facts.source_label(),
                    "chain_facts_live": facts.is_live(),
                    "network_id": config.network_id,
                    "beacon": facts.beacon().map(|b| serde_json::json!({
                        "epoch": b.epoch, "seed": b.seed_hex, "current_epoch": b.current_epoch,
                        "observed_daa_score": b.observed_daa_score,
                    })).unwrap_or_else(|e| serde_json::json!({ "error": e })),
                }),
                None => serde_json::json!({
                    "enabled": false,
                    "chain_facts": "none — dev harness mode (no challenges, bonds, DA or arbitration)",
                    "chain_facts_live": false,
                }),
            };
            status["consensus_seams"] = seams;
            status["disputes"] = serde_json::json!(guard.disputes_json());
            Ok(status)
        }
        ("POST", "/palw/v1/providers") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("provider registration needs a chain-facts source")?;
            let registration: ProviderRegistrationV1 =
                serde_json::from_value(v).map_err(|e| format!("bad registration: {e}"))?;
            let provider = state.lock().unwrap().register_provider(&registration, chain.as_ref(), now)?;
            Ok(json!({
                "bond_outpoint": provider.bond_outpoint,
                "credential": provider.credential_hex,
                "session_valid_from_epoch": provider.session_valid_from_epoch,
                "session_valid_until_epoch": provider.session_valid_until_epoch,
            }))
        }),
        ("POST", "/palw/v1/challenges") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("challenge leasing needs a chain-facts source")?;
            let bond = v.get("provider_bond").and_then(|b| b.as_str()).ok_or("missing provider_bond")?;
            authenticate(request, state, config, bond)?;
            let prompt_ids: Vec<u32> = v
                .get("prompt_ids")
                .and_then(|p| p.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u32)).collect())
                .ok_or("missing prompt_ids")?;
            let max_new = v.get("max_new").and_then(|m| m.as_u64()).ok_or("missing max_new")? as u32;
            let shape_id = v.get("shape_id").and_then(|s| s.as_u64()).unwrap_or(1) as u16;
            let lease = state.lock().unwrap().lease_challenge(
                bond,
                &prompt_ids,
                max_new,
                crate::match_key::RUNTIME_CLASS_LABEL,
                shape_id,
                chain.as_ref(),
                now,
            )?;
            serde_json::to_value(lease).map_err(|e| e.to_string())
        }),
        ("GET", "/palw/v1/da/obligations") => {
            let Some(bond) = query_param(&request.query, "provider_bond") else {
                return (400, json!({ "error": { "message": "missing provider_bond" } }));
            };
            let chain = match chain.as_ref() {
                Some(c) => c,
                None => return (400, json!({ "error": { "message": "DA needs a chain-facts source" } })),
            };
            let mut guard = state.lock().unwrap();
            match guard.open_da_challenges(&bond, chain.as_ref(), now) {
                Ok(_) => Ok(json!({ "obligations": guard.da_obligations_for(&bond) })),
                Err(e) => Err(e),
            }
        }
        ("POST", "/palw/v1/da/responses") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("DA needs a chain-facts source")?;
            let response: DaResponseWire = serde_json::from_value(v).map_err(|e| format!("bad DA response: {e}"))?;
            authenticate(request, state, config, &response.provider_bond)?;
            state.lock().unwrap().answer_da_challenge(&response, chain.as_ref(), now)?;
            Ok(json!({ "satisfied": true }))
        }),
        ("POST", "/palw/v1/da/sweep") => {
            let chain = match chain.as_ref() {
                Some(c) => c,
                None => return (400, json!({ "error": { "message": "DA needs a chain-facts source" } })),
            };
            state.lock().unwrap().sweep_da_timeouts(chain.as_ref(), now).map(|ids| json!({ "timed_out": ids }))
        }
        ("GET", "/palw/v1/audits") => {
            let Some(bond) = query_param(&request.query, "auditor_bond") else {
                return (400, json!({ "error": { "message": "missing auditor_bond" } }));
            };
            let guard = state.lock().unwrap();
            let assignments: Vec<Value> = guard
                .audit_assignments_for(&bond)
                .into_iter()
                .map(|(dispute, prompt_ids, max_new)| json!({
                    "dispute": dispute, "prompt_ids": prompt_ids, "max_new": max_new,
                }))
                .collect();
            Ok(json!({ "audits": assignments }))
        }
        ("POST", "/palw/v1/audits/verdicts") => parse_body().and_then(|v| {
            let dispute_id = v.get("dispute_id").and_then(|d| d.as_str()).ok_or("missing dispute_id")?;
            let auditor = v.get("auditor_bond").and_then(|a| a.as_str()).ok_or("missing auditor_bond")?;
            authenticate(request, state, config, auditor)?;
            let output_root = v.get("output_root").and_then(|o| o.as_str()).ok_or("missing output_root")?;
            let roots: RuntimeRootsV1 = v
                .get("runtime_roots")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("bad runtime_roots: {e}"))?
                .ok_or("missing runtime_roots")?;
            let evidence =
                state.lock().unwrap().adjudicate_dispute(dispute_id, auditor, output_root, &roots, now)?;
            serde_json::to_value(evidence).map_err(|e| e.to_string())
        }),
        ("POST", "/palw/v1/jobs") => parse_body().and_then(|v| {
            let submission: JobSubmissionV1 =
                serde_json::from_value(v).map_err(|e| format!("bad submission: {e}"))?;
            authenticate(request, state, config, &submission.provider_id)?;
            let mut guard = state.lock().unwrap();
            if config.require_bonded {
                let chain = chain.as_ref().ok_or("bonded mode needs a chain-facts source")?;
                let beacon = chain.beacon()?;
                // Seam 1: the challenge must be one we leased, for this prompt, to this provider.
                guard.check_lease(
                    &submission,
                    &submission.provider_id,
                    crate::match_key::RUNTIME_CLASS_LABEL,
                    beacon.current_epoch,
                )?;
            }
            guard.submit_job(&submission, now)?;
            // Seam 3: register the submitter's DA obligations over the context object.
            let mut da = Vec::new();
            if config.require_bonded {
                let chain = chain.as_ref().ok_or("bonded mode needs a chain-facts source")?;
                let object = crate::da::ChatContextObjectV4 {
                    network_id: config.network_id,
                    job_challenge: crate::chain::parse_hash64(
                        submission.job_challenge.as_deref().ok_or("missing job_challenge")?,
                    )?,
                    class_label: crate::match_key::RUNTIME_CLASS_LABEL.to_vec(),
                    max_new: submission.max_new,
                    prompt_token_ids: submission.prompt_ids.clone(),
                    output_token_ids: submission.output_token_ids.clone().unwrap_or_default(),
                };
                let commitment = crate::da::DaCommitmentWire::from_commitment(&object.commitment()?);
                da = guard.register_da(&submission.job_id, &submission.provider_id, &commitment, chain.as_ref(), now)?;
            }
            Ok(json!({ "accepted": true, "da_obligations": da }))
        }),
        ("POST", "/palw/v1/verdicts") => parse_body().and_then(|v| {
            let job_ids: Vec<String> = v
                .get("job_ids")
                .and_then(|j| j.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let verdicts = state.lock().unwrap().fetch_verdicts(&job_ids, now)?;
            let list: Vec<Value> =
                verdicts.iter().map(|(id, v)| json!({ "job_id": id, "verdict": v.as_str() })).collect();
            Ok(json!({ "verdicts": list }))
        }),
        ("GET", "/palw/v1/assignments") => {
            let Some(provider) = query_param(&request.query, "provider_id") else {
                return (400, json!({ "error": { "message": "missing provider_id" } }));
            };
            state
                .lock()
                .unwrap()
                .fetch_assignments(&provider, now)
                .map(|assignments| json!({ "assignments": assignments }))
        }
        ("POST", path) if path.starts_with("/palw/v1/assignments/") && path.ends_with("/decline") => {
            let job = path
                .trim_start_matches("/palw/v1/assignments/")
                .trim_end_matches("/decline");
            if job.is_empty() || job.contains('/') {
                return (404, json!({ "error": { "message": "no such route" } }));
            }
            parse_body().and_then(|v| {
                let provider = v
                    .get("provider_id")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| "missing provider_id".to_string())?;
                let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                state.lock().unwrap().decline_assignment(job, provider, reason, now)?;
                Ok(json!({ "declined": true }))
            })
        }
        ("POST", "/palw/v1/replica-results") => parse_body().and_then(|v| {
            let result: ReplicaResultV1 =
                serde_json::from_value(v).map_err(|e| format!("bad replica result: {e}"))?;
            authenticate(request, state, config, &result.provider_id)?;
            let mut guard = state.lock().unwrap();
            let matched = guard.submit_replica_result(&result, now)?;
            // Seam 4: a k=2 disagreement opens a dispute and (if escalated) draws an auditor.
            let dispute = if !matched && config.require_bonded {
                let chain = chain.as_ref().ok_or("bonded mode needs a chain-facts source")?;
                guard.open_dispute(&result.job_id, chain.as_ref(), now)?
            } else {
                None
            };
            Ok(json!({ "recorded": true, "matched": matched, "dispute": dispute }))
        }),
        _ => return (404, json!({ "error": { "message": "no such route" } })),
    };
    match outcome {
        Ok(v) => (200, v),
        Err(e) => (400, json!({ "error": { "message": e } })),
    }
}
