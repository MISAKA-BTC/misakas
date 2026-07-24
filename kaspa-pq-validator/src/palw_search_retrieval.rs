//! Node-side retrieval service for node-anchored web-search snapshots (measurement gate).
//!
//! Executes ONE provider search under a pinned policy, converts the typed outcome — success,
//! empty, timeout, HTTP failure, malformed payload, or egress denial — into a canonical
//! `SearchSnapshotV1`, and hands the caller the exact bytes to submit through
//! `ConsensusApi::palw_admit_search_snapshot`. Workers never see the provider; they consume the
//! admitted snapshot bytes.
//!
//! Egress rules in this build (fail-closed):
//! * `https://` provider endpoints are refused — the workspace pins tokio 1.42, which excludes
//!   the vetted TLS clients; refusing beats quietly shipping an unreviewed TLS stack.
//! * `http://` provider endpoints must resolve exclusively to loopback / RFC-1918 / CGNAT
//!   (100.64/10) addresses — the operator's own SearXNG — and only when the policy allows
//!   private providers. Plaintext queries to the public internet are never sent.
//! * Result URLs are recorded, never fetched here; body fetch needs the TLS gate first.
//! * Redirects are not followed; a redirect status is a typed HTTP failure.
//! * When the endpoint host is a DNS name, it is resolved once, every address is
//!   class-checked, and the connection is made to the pinned resolved address, so a
//!   rebinding name cannot re-point the request between check and connect.

use std::io::Read;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use kaspa_consensus_core::palw::search_snapshot::{
    PALW_SEARCH_MAX_RESULTS, PALW_SEARCH_MAX_SNIPPET_BYTES, PALW_SEARCH_MAX_TITLE_BYTES, PALW_SEARCH_MAX_URL_BYTES,
    PALW_SEARCH_SNAPSHOT_VERSION_V1, PalwSearchMediaTypeV1, PalwSearchOutcomeV1, PalwSearchProviderPolicyV1,
    PalwSearchResultV1, PalwSearchSnapshotV1, normalize_query_v1,
};
use kaspa_consensus_core::Hash64;

/// Default provider timeout.
pub const DEFAULT_TIMEOUT_MS: u64 = 15_000;
/// Default provider-response size cap.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// Default freshness window granted to a snapshot.
pub const DEFAULT_FRESHNESS_WINDOW_MS: u64 = 600_000;

/// Operator retrieval policy.
#[derive(Clone, Debug)]
pub struct RetrievalPolicyV1 {
    /// Permit `http://` providers on loopback/private/CGNAT addresses (local SearXNG).
    pub allow_private_provider: bool,
    /// Whole-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Provider response byte cap.
    pub max_response_bytes: usize,
    /// Ranked results kept (further rows are dropped deterministically).
    pub max_results: usize,
}

impl Default for RetrievalPolicyV1 {
    fn default() -> Self {
        Self {
            allow_private_provider: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_results: PALW_SEARCH_MAX_RESULTS,
        }
    }
}

/// Node-derived binding facts for the snapshot (network, genesis, DAA, wall clock).
#[derive(Clone, Copy, Debug)]
pub struct NodeAnchorV1 {
    /// Numeric network suffix.
    pub network_id: u32,
    /// Genesis hash of the anchoring network.
    pub genesis_hash: Hash64,
    /// Virtual DAA score observed at retrieval.
    pub daa_score: u64,
    /// Node wall clock, Unix milliseconds.
    pub unix_millis: u64,
}

/// One retrieval request.
#[derive(Clone, Debug)]
pub struct RetrievalRequestV1 {
    /// Provider endpoint, e.g. `http://127.0.0.1:8080`.
    pub endpoint: String,
    /// Query exactly as assigned by the scheduler.
    pub query: String,
    /// Ruleset pin recorded in the snapshot.
    pub ruleset_id: String,
    /// Scheduler assignment id (zero = unassigned diagnostic).
    pub assignment_id: Hash64,
    /// Hash of the provider policy document.
    pub provider_policy_id: Hash64,
    /// Language setting sent to the provider.
    pub language: String,
    /// Region tag recorded in the snapshot.
    pub region: String,
    /// Safe-search level 0..=2.
    pub safe_search: u8,
    /// Freshness window granted from retrieval time, milliseconds.
    pub freshness_window_millis: u64,
}

/// Returns `Err(reason)` when `ip` is not a public unicast address.
pub fn require_public(ip: IpAddr) -> Result<(), &'static str> {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            if v4.is_unspecified() {
                Err("unspecified address")
            } else if v4.is_loopback() {
                Err("loopback address")
            } else if v4.is_private() {
                Err("private (RFC 1918) address")
            } else if octets[0] == 100 && (64..128).contains(&octets[1]) {
                Err("shared/CGNAT (100.64/10) address")
            } else if v4.is_link_local() {
                Err("link-local address")
            } else if v4.is_broadcast() || v4.is_documentation() {
                Err("broadcast/documentation address")
            } else if octets[0] == 192 && octets[1] == 0 && octets[2] == 0 {
                Err("IETF protocol assignment (192.0.0/24) address")
            } else if octets[0] == 198 && (18..20).contains(&octets[1]) {
                Err("benchmarking (198.18/15) address")
            } else if v4.is_multicast() {
                Err("multicast address")
            } else if octets[0] >= 240 {
                Err("reserved (240/4) address")
            } else {
                Ok(())
            }
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return require_public(IpAddr::V4(mapped));
            }
            let segments = v6.segments();
            if v6.is_unspecified() || v6.is_loopback() {
                Err("loopback/unspecified address")
            } else if segments[0] & 0xfe00 == 0xfc00 {
                Err("unique-local (fc00::/7) address")
            } else if segments[0] & 0xffc0 == 0xfe80 {
                Err("link-local (fe80::/10) address")
            } else if segments[0] & 0xff00 == 0xff00 {
                Err("multicast address")
            } else {
                Ok(())
            }
        }
    }
}

/// Returns `Err(reason)` when `ip` is not loopback / RFC-1918 / CGNAT — the only classes a
/// plaintext-HTTP provider endpoint may live in.
pub fn require_operator_local(ip: IpAddr) -> Result<(), &'static str> {
    let allowed = match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            v4.is_loopback() || v4.is_private() || (octets[0] == 100 && (64..128).contains(&octets[1]))
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return require_operator_local(IpAddr::V4(mapped));
            }
            v6.is_loopback() || v6.segments()[0] & 0xfe00 == 0xfc00
        }
    };
    if allowed { Ok(()) } else { Err("plaintext provider endpoints must be loopback/private/CGNAT") }
}

/// A validated provider endpoint with its pinned connect addresses.
#[derive(Clone, Debug)]
pub struct ValidatedProviderEndpoint {
    host: String,
    port: u16,
    /// Every resolved address, class-checked; the request connects to `addrs[0]`.
    addrs: Vec<SocketAddr>,
}

fn split_host_port(authority: &str) -> Result<(String, u16), &'static str> {
    if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 literal: [addr]:port or [addr]
        let end = rest.find(']').ok_or("unterminated IPv6 literal")?;
        let host = &rest[..end];
        let port = match &rest[end + 1..] {
            "" => 80,
            tail => tail.strip_prefix(':').ok_or("malformed IPv6 authority")?.parse().map_err(|_| "bad port")?,
        };
        return Ok((host.to_string(), port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Ok((host.to_string(), port.parse().map_err(|_| "bad port")?)),
        _ => Ok((authority.to_string(), 80)),
    }
}

/// Parses and egress-checks a provider endpoint. HTTPS is refused in this build; HTTP must
/// resolve exclusively to operator-local address classes and be policy-enabled.
pub fn validate_provider_endpoint(endpoint: &str, policy: &RetrievalPolicyV1) -> Result<ValidatedProviderEndpoint, &'static str> {
    if endpoint.starts_with("https://") {
        return Err("https egress requires a vetted TLS client (workspace tokio pin); not available in this build");
    }
    let Some(rest) = endpoint.strip_prefix("http://") else {
        return Err("provider endpoint must be http:// or https://");
    };
    if !policy.allow_private_provider {
        return Err("plaintext http providers are disabled by policy");
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err("provider endpoint must not carry credentials and must name a host");
    }
    let (host, port) = split_host_port(authority)?;
    let addrs: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        (host.as_str(), port).to_socket_addrs().map_err(|_| "provider host did not resolve")?.collect()
    };
    if addrs.is_empty() {
        return Err("provider host resolved to no addresses");
    }
    for addr in &addrs {
        require_operator_local(addr.ip())?;
    }
    Ok(ValidatedProviderEndpoint { host, port, addrs })
}

/// UTF-8-safe truncation to at most `max_bytes` (never splits a code point).
#[must_use]
pub fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(*byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

enum TransportFailure {
    Timeout,
    Http(u16),
    Oversize,
}

fn http_get_bounded(
    endpoint: &ValidatedProviderEndpoint,
    path_and_query: &str,
    timeout_ms: u64,
    max_bytes: usize,
) -> Result<Vec<u8>, TransportFailure> {
    let pinned = endpoint.addrs[0];
    // Connect to the pinned, class-checked address; carry the original name in the Host
    // header so a vhost provider still routes, and DNS cannot re-point the connection.
    let url = format!("http://{pinned}{path_and_query}");
    let response = attohttpc::get(&url)
        .timeout(Duration::from_millis(timeout_ms))
        .header(attohttpc::header::HOST, format!("{}:{}", endpoint.host, endpoint.port))
        .header(attohttpc::header::USER_AGENT, "misaka-palw-search-retrieval/1")
        .header(attohttpc::header::ACCEPT, "application/json")
        .follow_redirects(false)
        .send()
        .map_err(|_| TransportFailure::Timeout)?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(TransportFailure::Http(status));
    }
    let (_, _, reader) = response.split();
    let mut body = Vec::with_capacity(64 * 1024);
    let mut bounded = reader.take(max_bytes as u64 + 1);
    bounded.read_to_end(&mut body).map_err(|_| TransportFailure::Timeout)?;
    if body.len() > max_bytes {
        return Err(TransportFailure::Oversize);
    }
    Ok(body)
}

fn parse_searxng_results(body: &[u8], max_results: usize) -> Result<Vec<PalwSearchResultV1>, &'static str> {
    let payload: serde_json::Value = serde_json::from_slice(body).map_err(|_| "provider payload is not JSON")?;
    let rows = payload.get("results").and_then(|value| value.as_array()).ok_or("provider payload has no results array")?;
    let mut results = Vec::new();
    for row in rows {
        if results.len() >= max_results.min(PALW_SEARCH_MAX_RESULTS) {
            break;
        }
        let Some(url) = row.get("url").and_then(|value| value.as_str()) else { continue };
        if !(url.starts_with("https://") || url.starts_with("http://")) || url.len() > PALW_SEARCH_MAX_URL_BYTES {
            continue;
        }
        // Defense-in-depth for future body fetchers: drop rows whose host is a literal
        // non-public IP outright (no DNS here — resolving every row would leak the query
        // to a resolver; named hosts are re-checked at fetch time behind the TLS gate).
        let authority = url.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
        if let Ok((host, _)) = split_host_port(authority)
            && let Ok(ip) = host.parse::<IpAddr>()
            && require_public(ip).is_err()
        {
            continue;
        }
        let media_type = match row.get("category").and_then(|value| value.as_str()) {
            Some("news") => PalwSearchMediaTypeV1::News,
            Some("images") => PalwSearchMediaTypeV1::Image,
            Some("general") | None => PalwSearchMediaTypeV1::Web,
            Some(_) => PalwSearchMediaTypeV1::Other,
        };
        results.push(PalwSearchResultV1 {
            rank: (results.len() + 1) as u16,
            media_type,
            title: bounded_text(row.get("title").and_then(|value| value.as_str()).unwrap_or(""), PALW_SEARCH_MAX_TITLE_BYTES),
            url: url.to_string(),
            snippet: bounded_text(
                row.get("content").and_then(|value| value.as_str()).unwrap_or(""),
                PALW_SEARCH_MAX_SNIPPET_BYTES,
            ),
        });
    }
    Ok(results)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// Runs one policy-bounded provider search and returns the canonical snapshot. Provider
/// failures are typed outcomes inside the snapshot; `Err` is reserved for local misuse
/// (empty query, snapshot bounds violated by configuration).
pub fn retrieve_search_snapshot(
    request: &RetrievalRequestV1,
    anchor: &NodeAnchorV1,
    policy: &RetrievalPolicyV1,
) -> Result<PalwSearchSnapshotV1, String> {
    let normalized_query = normalize_query_v1(&request.query);
    if normalized_query.is_empty() {
        return Err("query must not be empty after normalization".to_string());
    }
    let (outcome, results) = match validate_provider_endpoint(&request.endpoint, policy) {
        Err(_) => (PalwSearchOutcomeV1::EgressDenied, vec![]),
        Ok(endpoint) => {
            let path_and_query = format!(
                "/search?q={}&format=json&language={}&safesearch={}",
                percent_encode(&normalized_query),
                percent_encode(&request.language),
                request.safe_search.min(2),
            );
            match http_get_bounded(&endpoint, &path_and_query, policy.timeout_ms, policy.max_response_bytes) {
                Err(TransportFailure::Timeout) => (PalwSearchOutcomeV1::ProviderTimeout, vec![]),
                Err(TransportFailure::Http(status)) => (PalwSearchOutcomeV1::ProviderHttpFailure { status }, vec![]),
                Err(TransportFailure::Oversize) => (PalwSearchOutcomeV1::ProviderMalformed, vec![]),
                Ok(body) => match parse_searxng_results(&body, policy.max_results) {
                    Err(_) => (PalwSearchOutcomeV1::ProviderMalformed, vec![]),
                    Ok(results) if results.is_empty() => (PalwSearchOutcomeV1::EmptyResults, vec![]),
                    Ok(results) => (PalwSearchOutcomeV1::Ok, results),
                },
            }
        }
    };
    let snapshot = PalwSearchSnapshotV1 {
        version: PALW_SEARCH_SNAPSHOT_VERSION_V1,
        network_id: anchor.network_id,
        genesis_hash: anchor.genesis_hash,
        ruleset_id: request.ruleset_id.clone(),
        assignment_id: request.assignment_id,
        original_query_sha256: sha256(request.query.as_bytes()),
        normalized_query_sha256: sha256(normalized_query.as_bytes()),
        original_query: request.query.clone(),
        normalized_query,
        provider: PalwSearchProviderPolicyV1 {
            provider_id: "searxng".to_string(),
            policy_id: request.provider_policy_id,
            region: request.region.clone(),
            language: request.language.clone(),
            safe_search: request.safe_search.min(2),
        },
        retrieval_unix_millis: anchor.unix_millis,
        retrieval_daa_score: anchor.daa_score,
        freshness_deadline_millis: anchor.unix_millis.saturating_add(request.freshness_window_millis.max(1)),
        outcome,
        results,
        bodies: vec![],
    };
    snapshot.validate().map_err(|error| format!("constructed snapshot failed validation: {error}"))?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    fn anchor() -> NodeAnchorV1 {
        NodeAnchorV1 {
            network_id: 111,
            genesis_hash: Hash64::from_bytes([0x42; 64]),
            daa_score: 10_000,
            unix_millis: 1_784_800_000_000,
        }
    }

    fn request(endpoint: String) -> RetrievalRequestV1 {
        RetrievalRequestV1 {
            endpoint,
            query: "量子  コンピュータ".to_string(),
            ruleset_id: "palw-search-v1".to_string(),
            assignment_id: Hash64::from_bytes([0; 64]),
            provider_policy_id: Hash64::from_bytes([0x99; 64]),
            language: "ja-JP".to_string(),
            region: "jp".to_string(),
            safe_search: 1,
            freshness_window_millis: 600_000,
        }
    }

    fn serve_once(status_line: &'static str, content_type: &'static str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut request);
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{addr}")
    }

    #[test]
    fn public_and_operator_local_classifiers_are_strict() {
        for public in ["93.184.216.34", "2606:2800:220:1::1"] {
            assert!(require_public(public.parse().unwrap()).is_ok(), "{public}");
            assert!(require_operator_local(public.parse().unwrap()).is_err(), "{public}");
        }
        for private in
            ["127.0.0.1", "10.1.2.3", "172.16.0.9", "192.168.1.1", "100.125.83.97", "169.254.1.1", "::1", "fd00::1", "fe80::1", "224.0.0.1", "240.0.0.1", "198.18.0.1", "::ffff:10.0.0.1"]
        {
            assert!(require_public(private.parse().unwrap()).is_err(), "{private}");
        }
        for local in ["127.0.0.1", "10.1.2.3", "100.125.83.97", "::1", "fd00::1", "::ffff:192.168.0.1"] {
            assert!(require_operator_local(local.parse().unwrap()).is_ok(), "{local}");
        }
        for non_local in ["169.254.1.1", "fe80::1", "93.184.216.34"] {
            assert!(require_operator_local(non_local.parse().unwrap()).is_err(), "{non_local}");
        }
    }

    #[test]
    fn provider_endpoint_gate_refuses_https_public_and_credentials() {
        let policy = RetrievalPolicyV1::default();
        assert!(validate_provider_endpoint("https://searx.example.org", &policy).is_err());
        assert!(validate_provider_endpoint("http://user:pw@127.0.0.1:8080", &policy).is_err());
        assert!(validate_provider_endpoint("http://93.184.216.34:8080", &policy).is_err());
        assert!(validate_provider_endpoint("ftp://127.0.0.1", &policy).is_err());
        assert!(validate_provider_endpoint("http://127.0.0.1:8080", &policy).is_ok());
        assert!(validate_provider_endpoint("http://[::1]:8080", &policy).is_ok());
        let closed = RetrievalPolicyV1 { allow_private_provider: false, ..RetrievalPolicyV1::default() };
        assert!(validate_provider_endpoint("http://127.0.0.1:8080", &closed).is_err());
    }

    #[test]
    fn bounded_text_never_splits_code_points() {
        assert_eq!(bounded_text("abcdef", 4), "abcd");
        let truncated = bounded_text("量子コンピュータ", 7);
        assert_eq!(truncated, "量子");
        assert!(truncated.len() <= 7);
        assert_eq!(bounded_text("short", 100), "short");
    }

    #[test]
    fn successful_search_yields_admissible_ok_snapshot() {
        let body = serde_json::json!({
            "results": [
                {"title": "量子コンピュータ - Wikipedia", "url": "https://ja.wikipedia.org/wiki/Q", "content": "重ね合わせ", "category": "general"},
                {"title": "news", "url": "https://example.org/n", "content": "x", "category": "news"},
                {"title": "dropped", "url": "javascript:alert(1)", "content": "bad scheme"},
                {"title": "dropped", "url": "http://10.0.0.7/steal", "content": "private literal IP"},
                {"title": "dropped", "url": "http://[fd00::1]:8080/x", "content": "unique-local literal IP"},
            ]
        })
        .to_string();
        let endpoint = serve_once("200 OK", "application/json", body);
        let snapshot = retrieve_search_snapshot(&request(endpoint), &anchor(), &RetrievalPolicyV1::default()).unwrap();
        assert_eq!(snapshot.outcome, PalwSearchOutcomeV1::Ok);
        assert_eq!(snapshot.results.len(), 2);
        assert_eq!(snapshot.results[0].rank, 1);
        assert_eq!(snapshot.results[1].media_type, PalwSearchMediaTypeV1::News);
        assert_eq!(snapshot.normalized_query, "量子 コンピュータ");
        // The snapshot round-trips through the strict consensus decoder and hashes stably.
        let bytes = snapshot.encode().unwrap();
        let decoded = PalwSearchSnapshotV1::decode_strict(&bytes).unwrap();
        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.da_commitment().unwrap().object_len as usize, bytes.len());
    }

    #[test]
    fn provider_failures_become_typed_first_class_snapshots() {
        let http_failure = serve_once("503 Service Unavailable", "text/plain", "down".to_string());
        let snapshot = retrieve_search_snapshot(&request(http_failure), &anchor(), &RetrievalPolicyV1::default()).unwrap();
        assert_eq!(snapshot.outcome, PalwSearchOutcomeV1::ProviderHttpFailure { status: 503 });
        assert!(snapshot.results.is_empty());
        assert!(snapshot.encode().is_ok());

        let malformed = serve_once("200 OK", "application/json", "not json".to_string());
        let snapshot = retrieve_search_snapshot(&request(malformed), &anchor(), &RetrievalPolicyV1::default()).unwrap();
        assert_eq!(snapshot.outcome, PalwSearchOutcomeV1::ProviderMalformed);

        let empty = serve_once("200 OK", "application/json", serde_json::json!({"results": []}).to_string());
        let snapshot = retrieve_search_snapshot(&request(empty), &anchor(), &RetrievalPolicyV1::default()).unwrap();
        assert_eq!(snapshot.outcome, PalwSearchOutcomeV1::EmptyResults);

        let denied = retrieve_search_snapshot(
            &request("https://searx.example.org".to_string()),
            &anchor(),
            &RetrievalPolicyV1::default(),
        )
        .unwrap();
        assert_eq!(denied.outcome, PalwSearchOutcomeV1::EgressDenied);

        let refused = retrieve_search_snapshot(
            &request("http://127.0.0.1:9".to_string()),
            &anchor(),
            &RetrievalPolicyV1 { timeout_ms: 1_500, ..RetrievalPolicyV1::default() },
        )
        .unwrap();
        assert_eq!(refused.outcome, PalwSearchOutcomeV1::ProviderTimeout);
    }

    #[test]
    fn redirects_are_denied_not_followed() {
        let redirect = serve_once("302 Found", "text/plain", String::new());
        let snapshot = retrieve_search_snapshot(&request(redirect), &anchor(), &RetrievalPolicyV1::default()).unwrap();
        assert_eq!(snapshot.outcome, PalwSearchOutcomeV1::ProviderHttpFailure { status: 302 });
    }
}
