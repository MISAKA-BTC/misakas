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

use crate::state::BridgeState;
use crate::wire::{JobSubmissionV1, ReplicaResultV1};

const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Body cap: prompt_ids for a 32k-token context serialize to well under 1 MiB; 16 MiB leaves
/// room without inviting abuse.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 128;
const CONN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct HttpConfig {
    pub listen: SocketAddr,
    pub auth_token: Option<String>,
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
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().map_err(|_| "bad content-length".to_string())?;
        } else if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.to_string());
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
    Ok(ParsedRequest { method, path, query, authorization, body })
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

    let (code, body) = dispatch(&request, &state);
    write_response(&mut stream, code, &body).await;
}

fn dispatch(request: &ParsedRequest, state: &Mutex<BridgeState>) -> (u16, Value) {
    let now = now_unix_ms();
    let parse_body = || -> Result<Value, String> {
        if request.body.is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_slice(&request.body).map_err(|e| format!("bad json: {e}"))
    };
    let outcome: Result<Value, String> = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/palw/v1/status") => Ok(state.lock().unwrap().status_json()),
        ("POST", "/palw/v1/jobs") => parse_body().and_then(|v| {
            let submission: JobSubmissionV1 =
                serde_json::from_value(v).map_err(|e| format!("bad submission: {e}"))?;
            state.lock().unwrap().submit_job(&submission, now)?;
            Ok(json!({ "accepted": true }))
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
            let matched = state.lock().unwrap().submit_replica_result(&result, now)?;
            Ok(json!({ "recorded": true, "matched": matched }))
        }),
        _ => return (404, json!({ "error": { "message": "no such route" } })),
    };
    match outcome {
        Ok(v) => (200, v),
        Err(e) => (400, json!({ "error": { "message": e } })),
    }
}
