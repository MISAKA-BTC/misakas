//! The bridge's HTTP surface — a hand-written HTTP/1.1 server over raw tokio, the same house
//! pattern as `mtp/service/src/http.rs` and `rpc/eth` (the workspace pins tokio 1.42.1, which
//! rules out axum/hyper): `TcpListener` + a connection semaphore + a whole-connection timeout +
//! head/body caps + `Connection: close`.
//!
//! Routes (the palw-gateway coordinator protocol v1, served under `/palw/v1`):
//! * `POST /palw/v1/jobs`                          → `{accepted:true}` (idempotent by job_id)
//! * `POST /palw/v1/verdicts` `{job_ids:[…]}`      → `{verdicts:[{job_id,verdict}]}`
//! * `GET  /palw/v1/assignments?provider_id=X`     → `{assignments:[…]}` (beacon-DRAWN, §SEL-01)
//! * `POST /palw/v1/assignments/{job}/decline`     → `{declined:true}`
//! * `POST /palw/v1/replica-results`               → `{recorded:true, matched:bool}`
//! * `POST /palw/v1/pcpb/self-flows`               → Seam 5: open a self-serial flow → current step
//! * `GET  /palw/v1/pcpb/self-flows?a_commit=&provider_bond=` → poll/advance the flow
//! * `POST /palw/v1/pcpb/self-flows/receipt`       → B's signed receipt → produced witness
//! * `POST /palw/v1/pcpb/witnesses` `{job_challenge}` → external witness for a Seam-1 lease
//! * `GET  /palw/v1/pcpb/witnesses?leaf_challenge=X`  → fetch a produced witness
//! * `GET  /palw/v1/status`                        → journal head/seq, job phases, providers
//! * `GET  /health`                                → liveness (always unauthenticated)
//!
//! With `--auth-token`, every `/palw/v1/*` request must carry `Authorization: Bearer <token>`.
//!
//! **Provider authentication (BRIDGE-AUTH-01).** In bonded mode EVERY `/palw/v1/*` route that
//! names a provider authenticates it, the read routes included — identity used to travel as a
//! bare query parameter on routes that never checked a signature, so naming another provider's
//! bond was enough to claim its assignments, read its audit prompts, or open its DA obligations.
//! A request carries `X-Palw-Signature` over `SignedRequest` (network, bond, method, path,
//! CANONICAL QUERY, body, nonce, expiry) plus `X-Palw-Nonce` and `X-Palw-Expires`; the bridge
//! spends the nonce, so one signature is good for exactly one request inside its window.
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
use crate::provider::{ProviderRegistrationV1, SignedRequest, body_digest, canonical_query};
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
    /// `X-Palw-Signature`: the provider's ML-DSA-87 signature over this WHOLE request.
    provider_signature: Option<String>,
    /// `X-Palw-Nonce`: single-use value making one signature usable once.
    provider_nonce: Option<String>,
    /// `X-Palw-Expires`: unix ms after which the signature is refused.
    provider_expires: Option<i64>,
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
    let mut provider_nonce = None;
    let mut provider_expires = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().map_err(|_| "bad content-length".to_string())?;
        } else if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("x-palw-signature") {
            provider_signature = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("x-palw-nonce") {
            provider_nonce = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("x-palw-expires") {
            provider_expires = Some(value.parse().map_err(|_| "bad x-palw-expires".to_string())?);
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
    Ok(ParsedRequest { method, path, query, authorization, provider_signature, provider_nonce, provider_expires, body })
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

    // `dispatch` is synchronous and may block on a node round-trip; run it on the blocking pool
    // so a slow node cannot starve the reactor (and so blocking there is legal at all).
    let dispatch_state = Arc::clone(&state);
    let dispatch_config = Arc::clone(&config);
    let dispatched = tokio::task::spawn_blocking(move || dispatch(&request, &dispatch_state, &dispatch_config)).await;
    let (code, body) = match dispatched {
        Ok(pair) => pair,
        Err(e) => (500, json!({ "error": { "message": format!("dispatch panicked: {e}") } })),
    };
    write_response(&mut stream, code, &body).await;
}

/// Provider authentication for the routes that need it. Returns the authenticated bond
/// outpoint. In dev-harness mode (no chain facts, `--require-bonded` off) this is skipped and
/// the caller falls back to the self-declared id — which is exactly why `/palw/v1/status`
/// reports the mode.
/// Authenticate one request against the bond it declares, binding method, path, canonical query,
/// body, nonce and expiry (BRIDGE-AUTH-01).
///
/// `declared_bond` is whatever the request SAYS it is — a query parameter or a body field. It is
/// only an index into the registry: what makes it the caller's identity is that the signature is
/// over a preimage containing it, so claiming another provider's bond fails verification under
/// that provider's session key. Before this, the read routes never called here at all, and the
/// signature would not have covered the query the identity travelled in even if they had.
fn authenticate(
    request: &ParsedRequest,
    state: &Mutex<BridgeState>,
    config: &HttpConfig,
    declared_bond: &str,
    now_unix_ms: i64,
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
    let signature =
        request.provider_signature.as_deref().ok_or("missing X-Palw-Signature (this bridge requires bonded, signed providers)")?;
    let nonce = request.provider_nonce.as_deref().ok_or("missing X-Palw-Nonce")?;
    let expires = request.provider_expires.ok_or("missing X-Palw-Expires")?;
    let signed = SignedRequest {
        network_id: config.network_id,
        bond_outpoint: declared_bond,
        method: &request.method,
        path: &request.path,
        canonical_query: &canonical_query(&request.query),
        body_digest: &body_digest(&request.body),
        nonce,
        expires_at_unix_ms: expires,
    };
    let mut guard = state.lock().unwrap();
    let provider = guard.authenticate(&signed, signature, chain.as_ref(), now_unix_ms)?;
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
            let registration: ProviderRegistrationV1 = serde_json::from_value(v).map_err(|e| format!("bad registration: {e}"))?;
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
            authenticate(request, state, config, bond, now)?;
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
            // BRIDGE-AUTH-01: opening DA challenges against a bond is consequential — an
            // unauthenticated caller could start another provider's obligation clock.
            if let Err(e) = authenticate(request, state, config, &bond, now) {
                return (401, json!({ "error": { "message": e } }));
            }
            let mut guard = state.lock().unwrap();
            match guard.open_da_challenges(&bond, chain.as_ref(), now) {
                Ok(_) => Ok(json!({ "obligations": guard.da_obligations_for(&bond) })),
                Err(e) => Err(e),
            }
        }
        ("POST", "/palw/v1/da/responses") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("DA needs a chain-facts source")?;
            let response: DaResponseWire = serde_json::from_value(v).map_err(|e| format!("bad DA response: {e}"))?;
            authenticate(request, state, config, &response.provider_bond, now)?;
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
            // BRIDGE-AUTH-01: audit prompts are the auditor's own work item; anyone able to read
            // them by naming a bond can pre-compute the reference output they will be checked against.
            if let Err(e) = authenticate(request, state, config, &bond, now) {
                return (401, json!({ "error": { "message": e } }));
            }
            let guard = state.lock().unwrap();
            let assignments: Vec<Value> = guard
                .audit_assignments_for(&bond)
                .into_iter()
                .map(|(dispute, prompt_ids, max_new)| {
                    json!({
                        "dispute": dispute, "prompt_ids": prompt_ids, "max_new": max_new,
                    })
                })
                .collect();
            Ok(json!({ "audits": assignments }))
        }
        ("POST", "/palw/v1/audits/verdicts") => parse_body().and_then(|v| {
            let dispute_id = v.get("dispute_id").and_then(|d| d.as_str()).ok_or("missing dispute_id")?;
            let auditor = v.get("auditor_bond").and_then(|a| a.as_str()).ok_or("missing auditor_bond")?;
            authenticate(request, state, config, auditor, now)?;
            let output_root = v.get("output_root").and_then(|o| o.as_str()).ok_or("missing output_root")?;
            let roots: RuntimeRootsV1 = v
                .get("runtime_roots")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| format!("bad runtime_roots: {e}"))?
                .ok_or("missing runtime_roots")?;
            let evidence = state.lock().unwrap().adjudicate_dispute(dispute_id, auditor, output_root, &roots, now)?;
            serde_json::to_value(evidence).map_err(|e| e.to_string())
        }),
        ("POST", "/palw/v1/jobs") => parse_body().and_then(|v| {
            let submission: JobSubmissionV1 = serde_json::from_value(v).map_err(|e| format!("bad submission: {e}"))?;
            authenticate(request, state, config, &submission.provider_id, now)?;
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
                    job_challenge: crate::chain::parse_hash64(submission.job_challenge.as_deref().ok_or("missing job_challenge")?)?,
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
            let list: Vec<Value> = verdicts.iter().map(|(id, v)| json!({ "job_id": id, "verdict": v.as_str() })).collect();
            Ok(json!({ "verdicts": list }))
        }),
        ("GET", "/palw/v1/assignments") => {
            let Some(provider) = query_param(&request.query, "provider_id") else {
                return (400, json!({ "error": { "message": "missing provider_id" } }));
            };
            // BRIDGE-AUTH-01: `provider_id` used to be a bare query parameter on an
            // unauthenticated route — naming someone else's bond claimed their work.
            if let Err(e) = authenticate(request, state, config, &provider, now) {
                return (401, json!({ "error": { "message": e } }));
            }
            state
                .lock()
                .unwrap()
                .fetch_assignments(&provider, chain.as_deref(), now)
                .map(|assignments| json!({ "assignments": assignments }))
        }
        ("POST", path) if path.starts_with("/palw/v1/assignments/") && path.ends_with("/decline") => {
            let job = path.trim_start_matches("/palw/v1/assignments/").trim_end_matches("/decline");
            if job.is_empty() || job.contains('/') {
                return (404, json!({ "error": { "message": "no such route" } }));
            }
            parse_body().and_then(|v| {
                let provider = v.get("provider_id").and_then(|p| p.as_str()).ok_or_else(|| "missing provider_id".to_string())?;
                // BRIDGE-AUTH-01: declining is a denial-of-service primitive against the holder
                // of an assignment, so it must be the holder speaking.
                authenticate(request, state, config, provider, now)?;
                let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("");
                state.lock().unwrap().decline_assignment(job, provider, reason, now)?;
                Ok(json!({ "declined": true }))
            })
        }
        // ---- Seam 5 (ADR-0045 D3-b): PCPB evidence production ----
        // Self-serial flow: open (idempotent by a_commit) → poll → post B's signed receipt.
        ("POST", "/palw/v1/pcpb/self-flows") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("PCPB production needs a chain-facts source")?;
            let record: crate::pcpb::PcpbSelfFlowRecordV1 = serde_json::from_value(v).map_err(|e| format!("bad PCPB flow: {e}"))?;
            // Opening a flow claims seat A for this bond and starts an on-chain anchor lifecycle —
            // it must be the bond's holder speaking (BRIDGE-AUTH-01).
            authenticate(request, state, config, &record.a_bond, now)?;
            let step = state.lock().unwrap().open_pcpb_self_flow(&record, chain.as_ref(), now)?;
            serde_json::to_value(step).map_err(|e| e.to_string())
        }),
        ("GET", "/palw/v1/pcpb/self-flows") => {
            let (Some(a_commit), Some(bond)) = (query_param(&request.query, "a_commit"), query_param(&request.query, "provider_bond"))
            else {
                return (400, json!({ "error": { "message": "missing a_commit or provider_bond" } }));
            };
            let chain = match chain.as_ref() {
                Some(c) => c,
                None => return (400, json!({ "error": { "message": "PCPB production needs a chain-facts source" } })),
            };
            // The step leaks the flow's receipt preimage (B's signing bytes) — holder only.
            if let Err(e) = authenticate(request, state, config, &bond, now) {
                return (401, json!({ "error": { "message": e } }));
            }
            state
                .lock()
                .unwrap()
                .drive_pcpb_self_flow(&a_commit, &bond, chain.as_ref(), now)
                .and_then(|step| serde_json::to_value(step).map_err(|e| e.to_string()))
        }
        ("POST", "/palw/v1/pcpb/self-flows/receipt") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("PCPB production needs a chain-facts source")?;
            let field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from).ok_or_else(|| format!("missing {k}"));
            let a_commit = field("a_commit")?;
            let bond = field("provider_bond")?;
            authenticate(request, state, config, &bond, now)?;
            let produced = state.lock().unwrap().pcpb_partner_receipt(
                &a_commit,
                &bond,
                &field("b_ml_dsa_pk")?,
                &field("b_receipt_preimage")?,
                &field("b_signature")?,
                chain.as_ref(),
                now,
            )?;
            serde_json::to_value(produced).map_err(|e| e.to_string())
        }),
        // External branch: produce the witness for a Seam-1 lease (the lease is the challenge).
        ("POST", "/palw/v1/pcpb/witnesses") => parse_body().and_then(|v| {
            let chain = chain.as_ref().ok_or("PCPB production needs a chain-facts source")?;
            let challenge = v.get("job_challenge").and_then(|x| x.as_str()).ok_or("missing job_challenge")?;
            let bond = v.get("provider_bond").and_then(|x| x.as_str()).ok_or("missing provider_bond")?;
            authenticate(request, state, config, bond, now)?;
            let produced = state.lock().unwrap().produce_pcpb_external_witness(challenge, bond, chain.as_ref(), now)?;
            serde_json::to_value(produced).map_err(|e| e.to_string())
        }),
        ("GET", "/palw/v1/pcpb/witnesses") => {
            let Some(challenge) = query_param(&request.query, "leaf_challenge") else {
                return (400, json!({ "error": { "message": "missing leaf_challenge" } }));
            };
            let guard = state.lock().unwrap();
            match guard.pcpb_witness(&challenge) {
                Some(w) => serde_json::to_value(w).map_err(|e| e.to_string()),
                None => Err(format!("no produced witness for leaf challenge {challenge}")),
            }
        }
        ("POST", "/palw/v1/replica-results") => parse_body().and_then(|v| {
            let result: ReplicaResultV1 = serde_json::from_value(v).map_err(|e| format!("bad replica result: {e}"))?;
            authenticate(request, state, config, &result.provider_id, now)?;
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
