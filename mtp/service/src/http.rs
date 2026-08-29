//! Read-only self-serve query HTTP surface (ADR-0038 D3).
//!
//! A hand-written HTTP/1.1 server over raw tokio — the same house pattern as
//! `kaspa-eth-rpc` (`rpc/eth/src/lib.rs`): `TcpListener` + a connection semaphore
//! + a whole-connection timeout + a body/response cap + `Connection: close` +
//! permissive CORS. The workspace pins tokio 1.42.1, which rules out
//! axum/hyper/jsonrpsee, and the eth-rpc crate is the proven template. This
//! endpoint is **GET-only and unauthenticated**: it serves nothing that is not
//! already in a published, signed ledger.
//!
//! Routes (all under `/mtp/v1`):
//! * `GET /mtp/v1/points`          → the [`crate::query::Leaderboard`] — ALL ids' cumulative totals.
//! * `GET /mtp/v1/points/<id>`     → the [`crate::query::PointsView`] for an id.
//! * `GET /mtp/v1/epoch/<n>`       → the signed JSONL of epoch `n`'s latest issue.
//! * `GET /mtp/v1/epoch/<n>/facts` → the `EpochInput` facts sidecar (recompute source).
//! * `GET /mtp/v1/epoch/<n>/all`   → every issue's signed JSONL, latest first.
//! * `GET /mtp/v1/rules/<hash>`    → the `Rules` document whose hash is `<hash>`.
//! * `GET /mtp/v1/operator`        → the operator ML-DSA-87 pubkey (hex) + pins.
//! * `GET /health`                 → liveness.
//!
//! Every number served is byte-traceable to a signed file: the archive is
//! re-opened per request, so a query always reflects the currently-published set.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use misaka_mtp::Rules;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

use crate::publish::LedgerArchive;
use crate::query;

/// Defensive cap on a single request head (a GET has no body, but a slowloris can
/// still dribble headers).
const MAX_HEAD_BYTES: usize = 64 * 1024;
/// Max concurrent connections; excess are dropped (backpressure, not queued).
const MAX_CONNECTIONS: usize = 256;
/// Whole-connection deadline: read head + dispatch + write.
const CONN_TIMEOUT: Duration = Duration::from_secs(20);
/// Max serialized response bytes.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Immutable per-request context. `Arc`-shared across connection tasks; cheap to
/// clone (the archive itself is re-opened per request off `archive_dir`).
#[derive(Clone)]
pub struct HttpState {
    /// The `points/` archive directory (signed ledgers + index.json).
    pub archive_dir: PathBuf,
    /// The operator ML-DSA-87 verification key, hex (2592 B → 5184 hex chars).
    pub operator_pubkey_hex: String,
    /// The current rules document (served by `/rules/<hash>` when its hash matches).
    pub rules: Rules,
    /// Out-of-band operator-key pins surfaced by `/operator` (repo/misakascan/notes).
    pub operator_pins: Vec<String>,
}

impl HttpState {
    fn archive(&self) -> Result<LedgerArchive, String> {
        LedgerArchive::open(&self.archive_dir).map_err(|e| e.to_string())
    }
}

/// Serve the query API on `addr` until `shutdown` resolves. Mirrors
/// [`kaspa_eth_rpc::serve_with_shutdown`] connection handling.
pub async fn serve_with_shutdown<F>(addr: SocketAddr, state: Arc<HttpState>, shutdown: F) -> std::io::Result<()>
where
    F: std::future::Future<Output = ()> + Send,
{
    let listener = TcpListener::bind(addr).await?;
    log::info!("[mtp-http] MTP points query API listening on http://{addr}");
    let conn_sem = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tokio::pin!(shutdown);
    loop {
        let (stream, _peer) = tokio::select! {
            biased;
            _ = &mut shutdown => {
                log::info!("[mtp-http] shutdown received, stopping accept loop on {addr}");
                break;
            }
            accepted = listener.accept() => accepted?,
        };
        let Ok(permit) = conn_sem.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let state = state.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = serve_conn(stream, state).await {
                log::trace!("[mtp-http] connection error: {e}");
            }
        });
    }
    Ok(())
}

/// Serve the query API on `addr` forever (for callers owning the whole process).
pub async fn serve(addr: SocketAddr, state: Arc<HttpState>) -> std::io::Result<()> {
    serve_with_shutdown(addr, state, std::future::pending::<()>()).await
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

async fn serve_conn(mut stream: TcpStream, state: Arc<HttpState>) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let mut tmp = [0u8; 4096];

    let exchange = async {
        // Read the request head (up to CRLFCRLF), head-size-bounded.
        let header_end = loop {
            if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                break pos + 4;
            }
            if buf.len() > MAX_HEAD_BYTES {
                return write_response(&mut stream, 431, "Request Header Fields Too Large", "text/plain", "").await;
            }
            let n = stream.read(&mut tmp).await?;
            if n == 0 {
                return Ok(()); // client closed before a complete head
            }
            buf.extend_from_slice(&tmp[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
        let request_line = head.split("\r\n").next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let target = parts.next().unwrap_or("");

        if method.eq_ignore_ascii_case("OPTIONS") {
            return write_cors_preflight(&mut stream).await;
        }
        if !method.eq_ignore_ascii_case("GET") {
            return write_response(&mut stream, 405, "Method Not Allowed", "text/plain", "").await;
        }

        // Strip any query string / fragment; route on the path only.
        let path = target.split(['?', '#']).next().unwrap_or("");
        let (status, reason, ctype, body) = route(&state, path);
        if body.len() > MAX_RESPONSE_BYTES {
            let err = json!({ "error": "response too large; narrow the query" }).to_string();
            return write_response(&mut stream, 500, "Internal Server Error", "application/json", &err).await;
        }
        write_response(&mut stream, status, reason, ctype, &body).await
    };

    match tokio::time::timeout(CONN_TIMEOUT, exchange).await {
        Ok(r) => r,
        Err(_) => Ok(()), // timed out → drop
    }
}

/// Route a GET path to `(status, reason, content_type, body)`. Pure given the
/// archive on disk — factored out so it is unit-testable without a socket.
pub fn route(state: &HttpState, path: &str) -> (u16, &'static str, &'static str, String) {
    // health
    if path == "/health" || path == "/" {
        return ok_json(json!({ "service": "misaka-mtp-service", "status": "ok" }).to_string());
    }
    // operator pubkey + pins
    if path == "/mtp/v1/operator" {
        return ok_json(
            json!({
                "operator_pubkey_mldsa87_hex": state.operator_pubkey_hex,
                "pubkey_len_bytes": state.operator_pubkey_hex.len() / 2,
                "pins": state.operator_pins,
            })
            .to_string(),
        );
    }
    // points (leaderboard — ALL ids). Exact match, so it never shadows points/<id>.
    if path == "/mtp/v1/points" {
        let archive = match state.archive() {
            Ok(a) => a,
            Err(e) => return server_error(&e),
        };
        return match query::leaderboard(&archive) {
            Ok(v) => ok_json(serde_json::to_string(&v).unwrap_or_else(|_| "null".into())),
            Err(e) => server_error(&e.to_string()),
        };
    }
    // points/<id>
    if let Some(id) = path.strip_prefix("/mtp/v1/points/") {
        if id.is_empty() {
            return bad_request("missing id");
        }
        let archive = match state.archive() {
            Ok(a) => a,
            Err(e) => return server_error(&e),
        };
        return match query::points_view(&archive, id) {
            Ok(v) => ok_json(serde_json::to_string(&v).unwrap_or_else(|_| "null".into())),
            Err(query::QueryError::UnknownId) => not_found("no such id in any published epoch"),
            Err(e) => server_error(&e.to_string()),
        };
    }
    // epoch/<n>[/all]
    if let Some(rest) = path.strip_prefix("/mtp/v1/epoch/") {
        let archive = match state.archive() {
            Ok(a) => a,
            Err(e) => return server_error(&e),
        };
        // /epoch/<n>/facts → the EpochInput sidecar (D3 recompute source).
        if let Some(n_str) = rest.strip_suffix("/facts") {
            let Ok(n) = n_str.parse::<u64>() else {
                return bad_request("epoch must be an integer");
            };
            return match query::epoch_facts_jsonl(&archive, n) {
                Ok(facts) => ok_json(facts.trim_end().to_string()),
                Err(query::QueryError::UnknownEpoch(_)) => not_found("no such epoch"),
                Err(query::QueryError::NoFacts(_)) => not_found("epoch published ledger-only (no facts to recompute)"),
                Err(e) => server_error(&e.to_string()),
            };
        }
        let (n_str, all) = match rest.strip_suffix("/all") {
            Some(n) => (n, true),
            None => (rest, false),
        };
        let Ok(n) = n_str.parse::<u64>() else {
            return bad_request("epoch must be an integer");
        };
        if all {
            return match query::epoch_all_issues_jsonl(&archive, n) {
                Ok(lines) => {
                    // A JSON array of the raw signed JSONL strings (latest first).
                    let arr = Value::Array(lines.into_iter().map(Value::String).collect());
                    ok_json(arr.to_string())
                }
                Err(query::QueryError::UnknownEpoch(_)) => not_found("no such epoch"),
                Err(e) => server_error(&e.to_string()),
            };
        }
        return match query::epoch_jsonl(&archive, n) {
            // Byte-exact signed JSONL (one object); served as application/json.
            Ok(line) => ok_json(line.trim_end().to_string()),
            Err(query::QueryError::UnknownEpoch(_)) => not_found("no such epoch"),
            Err(e) => server_error(&e.to_string()),
        };
    }
    // rules/<hash>
    if let Some(hash) = path.strip_prefix("/mtp/v1/rules/") {
        let want = hash.trim().to_ascii_lowercase();
        let have = faster_hex::hex_string(&state.rules.rules_hash().as_bytes());
        if want == have {
            return ok_json(serde_json::to_string(&state.rules).unwrap_or_else(|_| "null".into()));
        }
        return not_found("no rules document with that hash is current");
    }
    not_found("unknown route")
}

fn ok_json(body: String) -> (u16, &'static str, &'static str, String) {
    (200, "OK", "application/json", body)
}
fn bad_request(msg: &str) -> (u16, &'static str, &'static str, String) {
    (400, "Bad Request", "application/json", json!({ "error": msg }).to_string())
}
fn not_found(msg: &str) -> (u16, &'static str, &'static str, String) {
    (404, "Not Found", "application/json", json!({ "error": msg }).to_string())
}
fn server_error(msg: &str) -> (u16, &'static str, &'static str, String) {
    (500, "Internal Server Error", "application/json", json!({ "error": msg }).to_string())
}

async fn write_response(stream: &mut TcpStream, status: u16, reason: &str, ctype: &str, body: &str) -> std::io::Result<()> {
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await
}

async fn write_cors_preflight(stream: &mut TcpStream) -> std::io::Result<()> {
    let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    stream.write_all(resp.as_bytes()).await?;
    stream.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pq_validator_core::ValidatorKey;
    use misaka_mtp::{EpochLedger, ScoreRow};
    use std::path::PathBuf;

    fn tempdir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let uniq = format!("mtp-http-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
        let p = std::env::temp_dir().join(uniq);
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn state_with_ledger() -> (HttpState, ValidatorKey) {
        let dir = tempdir();
        let key = ValidatorKey::from_seed([5; 32]);
        let mut a = LedgerArchive::open(&dir).unwrap();
        let mut l = EpochLedger {
            epoch: 3,
            range: ["s".into(), "e".into()],
            network: "testnet-10".into(),
            rules_hash: faster_hex::hex_string(&Rules::default().rules_hash().as_bytes()),
            inputs_hash: "bb".into(),
            scores: vec![ScoreRow { id: "gh:alice".into(), c1: 100, c2: 0, c3: 0, c4: 0, c5: 0, evidence: vec!["ev".into()] }],
            sig_mldsa87: None,
        };
        l.sign(&key);
        a.publish(&l, "", "").unwrap();
        let state = HttpState {
            archive_dir: dir,
            operator_pubkey_hex: faster_hex::hex_string(key.public_key()),
            rules: Rules::default(),
            operator_pins: vec!["repo:points/OPERATOR_PUBKEY".into()],
        };
        (state, key)
    }

    #[test]
    fn routes_points_epoch_rules_operator_health() {
        let (state, _key) = state_with_ledger();

        let (code, _, ct, body) = route(&state, "/mtp/v1/points/gh:alice");
        assert_eq!(code, 200);
        assert_eq!(ct, "application/json");
        assert!(body.contains("\"total\":100"));

        let (code, ..) = route(&state, "/mtp/v1/points/gh:nobody");
        assert_eq!(code, 404);

        // leaderboard (ALL ids) — the exact `/mtp/v1/points` path, never shadowed by points/<id>.
        let (code, _, _, body) = route(&state, "/mtp/v1/points");
        assert_eq!(code, 200);
        assert!(body.contains("\"entries\""));
        assert!(body.contains("\"participants\""));
        assert!(body.contains("gh:alice"));

        let (code, _, _, body) = route(&state, "/mtp/v1/epoch/3");
        assert_eq!(code, 200);
        assert!(body.contains("\"epoch\":3"));
        assert!(body.contains("sig_mldsa87"));

        let (code, ..) = route(&state, "/mtp/v1/epoch/9");
        assert_eq!(code, 404);
        let (code, ..) = route(&state, "/mtp/v1/epoch/not-a-number");
        assert_eq!(code, 400);

        let good_hash = faster_hex::hex_string(&Rules::default().rules_hash().as_bytes());
        let (code, _, _, body) = route(&state, &format!("/mtp/v1/rules/{good_hash}"));
        assert_eq!(code, 200);
        // Pinned to the constant, not a literal: a RULES_VERSION bump is a deliberate act and
        // should not also require chasing a hard-coded number in an unrelated route test.
        assert!(body.contains(&format!("\"version\":{}", misaka_mtp::rules::RULES_VERSION)));
        let (code, ..) = route(&state, "/mtp/v1/rules/deadbeef");
        assert_eq!(code, 404);

        let (code, _, _, body) = route(&state, "/mtp/v1/operator");
        assert_eq!(code, 200);
        assert!(body.contains("operator_pubkey_mldsa87_hex"));

        let (code, _, _, body) = route(&state, "/health");
        assert_eq!(code, 200);
        assert!(body.contains("ok"));

        let (code, ..) = route(&state, "/nonsense");
        assert_eq!(code, 404);
    }
}
