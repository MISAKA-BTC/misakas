//! **RFC-0009 stage B, the network half: an independent DA transport adapter.**
//!
//! [`crate::evidence`] fixes WHAT is served (an `EvidenceManifestV1` and its chunks, checked against the claim's own roots) and ships the dumbest
//! transport, a directory. This module adds what a miner who switches its PC off needs and the node, the Panel and a public verifier all share:
//!
//! * **providers over a network** — the same content-addressed layout (`claims/<claim>.manifest`, `chunks/<manifest id>/<n>.chunk`) over
//!   HTTP: [`HttpProvider`] reads and writes it, [`server`] is a reference provider that verifies what it is given. `https://` goes through
//!   `curl` ([`CurlHttp`]); no TLS stack is linked here.
//! * **a provider list** from a file or a flag ([`parse_provider_list_v1`]). There is NO on-chain discovery: that is a design gap, recorded.
//! * **upload with read-back** ([`publish_to_providers_v1`]) — a provider's ACK is not availability, so each upload is fetched back and verified.
//! * **availability** ([`check_availability_v1`]) — per provider, per chunk: verified / missing / corrupt / unreachable, from this machine's own
//!   observation. It is never proof for anybody else and never a slash reason: `Unreachable` is a local failure.
//! * **repair and retention** ([`repair_v1`], [`RetentionMonitor`]) — anyone holding verified bytes may re-seed a thin provider, so the evidence
//!   outlives the miner's uptime; the monitor ticks over watched claims until each claim's retention deadline.
//! * **one fetch for everyone** ([`fetch_claim_material_any`]) — the node (`evidence::fs::fetch_claim_material` is this over directories), the Panel
//!   and a public verifier read the same bytes, each chunk checked against a manifest that agrees with the CLAIM's roots.
//!
//! Nothing here moves a slash, changes a consensus rule or trusts a provider: a provider is a byte server, never a computation authority.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use kaspa_hashes::Hash64;

use crate::evidence::{
    ChunkProvider, ClaimRoots, EvidenceManifestV1, FetchReport, ManifestLimits, ProviderError, fetch_material_any, manifest_id_v1,
};

/// The word every availability line carries: what THIS machine saw, not something it can prove to anyone.
pub const LOCAL_OBSERVATION: &str = "LOCAL_OBSERVATION";

// ---------------------------------------------------------------------------------------------------------------------------------
// The provider list
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProviderSpecV1 {
    Dir(PathBuf),
    /// `http://host:port/prefix` or `https://…` (no trailing slash).
    Http(String),
}

impl ProviderSpecV1 {
    pub fn id(&self) -> String {
        match self {
            ProviderSpecV1::Dir(p) => p.display().to_string(),
            ProviderSpecV1::Http(u) => u.clone(),
        }
    }
}

/// Parse a provider list: one provider per line (or comma-separated on one line), `#` comments, blank lines ignored, duplicates removed. A
/// line is `dir:/path`, an absolute or relative path, or an `http(s)://` URL. Provider DISCOVERY is configuration only — nothing on chain names
/// providers yet (DESIGN_GAP): a miner chooses where to place evidence and a verifier chooses where to look.
pub fn parse_provider_list_v1(text: &str) -> Result<Vec<ProviderSpecV1>, String> {
    let mut out: Vec<ProviderSpecV1> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        for item in line.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let spec = if item.starts_with("http://") || item.starts_with("https://") {
                ProviderSpecV1::Http(item.trim_end_matches('/').to_string())
            } else if let Some(rest) = item.strip_prefix("dir:") {
                ProviderSpecV1::Dir(PathBuf::from(rest))
            } else if item.contains("://") {
                return Err(format!("provider {item:?}: only dir:, http:// and https:// providers exist"));
            } else {
                ProviderSpecV1::Dir(PathBuf::from(item))
            };
            if !out.contains(&spec) {
                out.push(spec);
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The provider trait: read, and write
// ---------------------------------------------------------------------------------------------------------------------------------

/// A byte-serving provider that can also be written to. Everything it returns is checked by the caller against the manifest and the claim.
pub trait EvidenceProvider: ChunkProvider {
    /// The manifest this provider holds for a claim. `Ok(None)`: it holds none. NOT trusted: the caller judges it against the claim.
    fn manifest_for(&self, claim_hex: &str) -> Result<Option<EvidenceManifestV1>, ProviderError>;
    fn put_chunk(&self, manifest_id: Hash64, index: u32, bytes: &[u8]) -> Result<(), ProviderError>;
    /// Written LAST: a reader must never see a manifest whose chunks are not all there.
    fn put_manifest(&self, claim_hex: &str, manifest: &EvidenceManifestV1) -> Result<(), ProviderError>;
}

fn decode_manifest(bytes: &[u8]) -> Result<EvidenceManifestV1, ProviderError> {
    let mut slice = bytes;
    let m = <EvidenceManifestV1 as borsh::BorshDeserialize>::deserialize(&mut slice)
        .map_err(|e| ProviderError(format!("the manifest does not decode: {e}")))?;
    if !slice.is_empty() {
        return Err(ProviderError("the manifest has trailing bytes".into()));
    }
    Ok(m)
}

impl EvidenceProvider for crate::evidence::fs::FsProvider {
    fn manifest_for(&self, claim_hex: &str) -> Result<Option<EvidenceManifestV1>, ProviderError> {
        match std::fs::read(crate::evidence::fs::manifest_path(&self.root, claim_hex)) {
            Ok(bytes) => decode_manifest(&bytes).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ProviderError(e.to_string())),
        }
    }
    fn put_chunk(&self, manifest_id: Hash64, index: u32, bytes: &[u8]) -> Result<(), ProviderError> {
        let path = crate::evidence::fs::chunk_path(&self.root, manifest_id, index);
        write_atomic(&path, bytes).map_err(|e| ProviderError(e.to_string()))
    }
    fn put_manifest(&self, claim_hex: &str, manifest: &EvidenceManifestV1) -> Result<(), ProviderError> {
        let path = crate::evidence::fs::manifest_path(&self.root, claim_hex);
        write_atomic(&path, &borsh::to_vec(manifest).expect("borsh-serializable")).map_err(|e| ProviderError(e.to_string()))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("partial");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// HTTP: a tiny client over std, and a curl fallback for https
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait HttpTransport: Send + Sync {
    /// `max_response` bounds the body this call will read: a provider cannot make a client buffer more than it asked for.
    fn request(&self, method: &str, url: &str, body: Option<&[u8]>, max_response: usize) -> Result<HttpResponse, String>;
}

/// Plain `http://` over `std::net`. `https://` is refused with a pointer to [`CurlHttp`].
pub struct StdHttp {
    pub timeout: Duration,
}

impl Default for StdHttp {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(20) }
    }
}

fn split_url(url: &str) -> Result<(String, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "this transport speaks http:// only (https:// needs the curl transport)".to_string())?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let host = if host.contains(':') { host.to_string() } else { format!("{host}:80") };
    Ok((host, path.to_string()))
}

impl HttpTransport for StdHttp {
    fn request(&self, method: &str, url: &str, body: Option<&[u8]>, max_response: usize) -> Result<HttpResponse, String> {
        let (host, path) = split_url(url)?;
        let addr: SocketAddr =
            host.to_socket_addrs().map_err(|e| format!("{host}: {e}"))?.next().ok_or_else(|| format!("{host}: no address"))?;
        let mut stream = TcpStream::connect_timeout(&addr, self.timeout).map_err(|e| format!("{host}: {e}"))?;
        stream.set_read_timeout(Some(self.timeout)).ok();
        stream.set_write_timeout(Some(self.timeout)).ok();
        let body = body.unwrap_or(&[]);
        let head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n", body.len());
        // A provider may refuse before it has read the body (413 on an oversized chunk): keep reading even if the write broke.
        let wrote = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
        // Read the response, never more than the header allowance plus what the caller asked for.
        let cap = max_response.saturating_add(16 * 1024);
        let mut raw = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let n = match stream.read(&mut buf) {
                Ok(n) => n,
                // A reset after an early refusal still leaves what was read; with nothing read, the write error is the better story.
                Err(e) if raw.is_empty() => {
                    return Err(format!("{host}: {}", wrote.err().map_or_else(|| e.to_string(), |w| w.to_string())));
                }
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&buf[..n]);
            if raw.len() > cap {
                return Err(format!("{host}: the response is larger than the {max_response} bytes asked for"));
            }
        }
        parse_response(&raw, max_response)
    }
}

fn parse_response(raw: &[u8], max_body: usize) -> Result<HttpResponse, String> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or("the response has no header end")?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| "the response header is not text")?;
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or("the response has no status line")?;
    let chunked =
        lines.any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked"));
    let body_raw = &raw[split + 4..];
    let body = if chunked { dechunk(body_raw)? } else { body_raw.to_vec() };
    if body.len() > max_body {
        return Err(format!("the response body is {} bytes, above the {max_body} asked for", body.len()));
    }
    Ok(HttpResponse { status, body })
}

fn dechunk(mut raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = raw.windows(2).position(|w| w == b"\r\n").ok_or("a chunked body ends mid-size-line")?;
        let size = usize::from_str_radix(
            std::str::from_utf8(&raw[..eol]).map_err(|_| "bad chunk size")?.split(';').next().unwrap_or("").trim(),
            16,
        )
        .map_err(|_| "bad chunk size")?;
        raw = &raw[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if raw.len() < size + 2 {
            return Err("a chunked body is truncated".into());
        }
        out.extend_from_slice(&raw[..size]);
        raw = &raw[size + 2..];
    }
}

/// `curl` as the transport, for `https://` (no TLS stack is linked into this crate). Requires `curl` on the host; its absence is a provider
/// failure, like any other.
pub struct CurlHttp {
    pub timeout_secs: u64,
}

impl Default for CurlHttp {
    fn default() -> Self {
        Self { timeout_secs: 30 }
    }
}

impl HttpTransport for CurlHttp {
    fn request(&self, method: &str, url: &str, body: Option<&[u8]>, max_response: usize) -> Result<HttpResponse, String> {
        use std::process::{Command, Stdio};
        let mut cmd = Command::new("curl");
        cmd.args([
            "-sS",
            "--max-time",
            &self.timeout_secs.to_string(),
            "--max-filesize",
            &max_response.to_string(),
            "-X",
            method,
            "-o",
            "-",
            "-w",
            "\n%{http_code}",
        ]);
        if body.is_some() {
            cmd.args(["--data-binary", "@-"]);
        }
        cmd.arg(url).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("cannot run curl: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Some(b) = body {
                stdin.write_all(b).map_err(|e| format!("curl: {e}"))?;
            }
        }
        let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
        if !out.status.success() {
            return Err(format!("curl: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        let split = out.stdout.iter().rposition(|b| *b == b'\n').ok_or("curl printed no status")?;
        let status = std::str::from_utf8(&out.stdout[split + 1..])
            .ok()
            .and_then(|s| s.trim().parse::<u16>().ok())
            .ok_or("curl printed no status")?;
        Ok(HttpResponse { status, body: out.stdout[..split].to_vec() })
    }
}

/// The transport a URL needs: plain http over std, https over curl.
pub fn transport_for(url: &str) -> Arc<dyn HttpTransport> {
    if url.starts_with("https://") { Arc::new(CurlHttp::default()) } else { Arc::new(StdHttp::default()) }
}

/// A provider that is an HTTP(S) base URL serving the content-addressed layout.
pub struct HttpProvider {
    pub base: String,
    pub http: Arc<dyn HttpTransport>,
    pub limits: ManifestLimits,
}

impl HttpProvider {
    pub fn new(base: impl Into<String>) -> Self {
        let base = base.into().trim_end_matches('/').to_string();
        let http = transport_for(&base);
        Self { base, http, limits: ManifestLimits::default() }
    }
    pub fn with_transport(base: impl Into<String>, http: Arc<dyn HttpTransport>) -> Self {
        Self { base: base.into().trim_end_matches('/').to_string(), http, limits: ManifestLimits::default() }
    }
}

const MANIFEST_MAX_BYTES: usize = 2 << 20;

impl ChunkProvider for HttpProvider {
    fn provider_id(&self) -> &str {
        &self.base
    }
    fn fetch_chunk(&self, manifest_id: Hash64, index: u32) -> Result<Vec<u8>, ProviderError> {
        let url = format!("{}/chunks/{manifest_id}/{index}.chunk", self.base);
        let r = self.http.request("GET", &url, None, self.limits.max_chunk_bytes as usize).map_err(ProviderError)?;
        if r.status != 200 {
            return Err(ProviderError(format!("HTTP {}", r.status)));
        }
        Ok(r.body)
    }
}

impl EvidenceProvider for HttpProvider {
    fn manifest_for(&self, claim_hex: &str) -> Result<Option<EvidenceManifestV1>, ProviderError> {
        let url = format!("{}/claims/{claim_hex}.manifest", self.base);
        let r = self.http.request("GET", &url, None, MANIFEST_MAX_BYTES).map_err(ProviderError)?;
        match r.status {
            200 => decode_manifest(&r.body).map(Some),
            404 => Ok(None),
            other => Err(ProviderError(format!("HTTP {other}"))),
        }
    }
    fn put_chunk(&self, manifest_id: Hash64, index: u32, bytes: &[u8]) -> Result<(), ProviderError> {
        let url = format!("{}/chunks/{manifest_id}/{index}.chunk", self.base);
        let r = self.http.request("PUT", &url, Some(bytes), 4096).map_err(ProviderError)?;
        if !(200..300).contains(&r.status) {
            return Err(ProviderError(format!("HTTP {}: {}", r.status, String::from_utf8_lossy(&r.body))));
        }
        Ok(())
    }
    fn put_manifest(&self, claim_hex: &str, manifest: &EvidenceManifestV1) -> Result<(), ProviderError> {
        let url = format!("{}/claims/{claim_hex}.manifest", self.base);
        let r = self
            .http
            .request("PUT", &url, Some(&borsh::to_vec(manifest).expect("borsh-serializable")), 4096)
            .map_err(ProviderError)?;
        if !(200..300).contains(&r.status) {
            return Err(ProviderError(format!("HTTP {}: {}", r.status, String::from_utf8_lossy(&r.body))));
        }
        Ok(())
    }
}

/// The providers a list names, as trait objects.
pub fn open_providers_v1(specs: &[ProviderSpecV1]) -> Vec<Box<dyn EvidenceProvider>> {
    specs
        .iter()
        .map(|spec| -> Box<dyn EvidenceProvider> {
            match spec {
                ProviderSpecV1::Dir(p) => Box::new(crate::evidence::fs::FsProvider::new(p.clone())),
                ProviderSpecV1::Http(u) => Box::new(HttpProvider::new(u.clone())),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------------------------
// One fetch for the node, the Panel and a public verifier
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimFetchErrorV1 {
    #[error(
        "no provider holds a manifest for this claim that agrees with the claim's own roots (offered {offered}, refused: {refusals:?}, silent: {silent:?})"
    )]
    NoAdmissibleManifest { offered: usize, refusals: Vec<String>, silent: Vec<String> },
    #[error("{0}")]
    Fetch(crate::evidence::FetchFailure),
}

/// **The claim's material from ANY provider** — directories, HTTP, a mix. Each provider's manifest is judged against the CLAIM (shape, network,
/// the three roots, chunk count, retention); the first admissible one wins; then every chunk comes from any provider and must verify against that
/// manifest. A provider that errors is silence, not evidence. The reassembled bytes go to the same root verification and re-execution a peer's
/// copy goes through: this adds a source, it replaces no check. `evidence::fs::fetch_claim_material` is this function over directories.
pub fn fetch_claim_material_any(
    providers: &[&dyn EvidenceProvider],
    claim_hex: &str,
    claim: &ClaimRoots,
    limits: &ManifestLimits,
    order_seed: Hash64,
) -> Result<(Vec<u8>, FetchReport), ClaimFetchErrorV1> {
    let (mut refusals, mut silent, mut offered, mut chosen) = (Vec::new(), Vec::new(), 0usize, None);
    for p in providers {
        match p.manifest_for(claim_hex) {
            Err(e) => silent.push(format!("{}: {e}", p.provider_id())),
            Ok(None) => {}
            Ok(Some(m)) => {
                offered += 1;
                match m.validate_shape(limits).and_then(|()| m.verify_claim_binding(claim)) {
                    Ok(()) => {
                        chosen = Some(m);
                        break;
                    }
                    Err(e) => refusals.push(format!("{}: {e}", p.provider_id())),
                }
            }
        }
    }
    let Some(manifest) = chosen else {
        return Err(ClaimFetchErrorV1::NoAdmissibleManifest { offered, refusals, silent });
    };
    let refs: Vec<&dyn ChunkProvider> = providers.iter().map(|p| *p as &dyn ChunkProvider).collect();
    let (chunks, report) = fetch_material_any(&manifest, claim, limits, &refs, order_seed).map_err(ClaimFetchErrorV1::Fetch)?;
    Ok((chunks.concat(), report))
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Upload with read-back
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutcomeV1 {
    pub provider: String,
    /// `Ok(())`: every chunk and the manifest were accepted AND read back verified. `Err`: why not (the provider may hold part of it).
    pub result: Result<(), String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReportV1 {
    pub manifest_id: Hash64,
    pub per_provider: Vec<PublishOutcomeV1>,
}

impl PublishReportV1 {
    pub fn verified_copies(&self) -> usize {
        self.per_provider.iter().filter(|o| o.result.is_ok()).count()
    }
}

/// **Place a claim's evidence with several providers.** For each: every chunk first, the manifest LAST, then the whole thing is read back and
/// verified — an ACK is the provider's word, a read-back that hashes to the manifest is what this machine saw. `Err` only when fewer than
/// `min_verified` providers hold a verified copy: the miner must not switch off on fewer.
pub fn publish_to_providers_v1(
    claim_hex: &str,
    manifest: &EvidenceManifestV1,
    chunks: &[Vec<u8>],
    providers: &[&dyn EvidenceProvider],
    min_verified: usize,
) -> Result<PublishReportV1, PublishReportV1> {
    let manifest_id = manifest_id_v1(manifest);
    let mut per_provider = Vec::new();
    for p in providers {
        let result = (|| -> Result<(), String> {
            for (i, bytes) in chunks.iter().enumerate() {
                p.put_chunk(manifest_id, i as u32, bytes).map_err(|e| format!("chunk {i}: {e}"))?;
            }
            p.put_manifest(claim_hex, manifest).map_err(|e| format!("manifest: {e}"))?;
            // Read back what the provider now serves.
            match p.manifest_for(claim_hex).map_err(|e| format!("read-back: {e}"))? {
                Some(m) if manifest_id_v1(&m) == manifest_id => {}
                Some(_) => return Err("read-back: the provider serves another manifest for this claim".into()),
                None => return Err("read-back: the provider serves no manifest after accepting it".into()),
            }
            for entry in &manifest.chunks {
                let bytes = p.fetch_chunk(manifest_id, entry.index).map_err(|e| format!("read-back chunk {}: {e}", entry.index))?;
                manifest.verify_chunk(entry.index, &bytes).map_err(|e| format!("read-back chunk {}: {e}", entry.index))?;
            }
            Ok(())
        })();
        per_provider.push(PublishOutcomeV1 { provider: p.provider_id().to_string(), result });
    }
    let report = PublishReportV1 { manifest_id, per_provider };
    if report.verified_copies() >= min_verified.max(1) { Ok(report) } else { Err(report) }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Availability, as this machine sees it
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkStateV1 {
    /// The provider served bytes that verify against the manifest (only reported when bytes were fetched).
    Verified,
    /// The provider answered that it holds the chunk, bytes not fetched ([`ProbeMode::Presence`]) — its word, not evidence.
    ClaimedPresent,
    Missing,
    /// The provider served bytes that do NOT match the manifest. Recorded; never accepted.
    Corrupt,
    /// No answer (connection, timeout). A local failure: says nothing about the provider's behaviour towards anyone else.
    Unreachable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestStandingV1 {
    Same,
    Absent,
    /// The provider holds a DIFFERENT manifest under this claim id. Not used; recorded.
    Different(Hash64),
    Unreachable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderAvailabilityV1 {
    pub provider: String,
    pub manifest: ManifestStandingV1,
    pub chunks: Vec<ChunkStateV1>,
}

impl ProviderAvailabilityV1 {
    pub fn verified_chunks(&self) -> usize {
        self.chunks.iter().filter(|c| **c == ChunkStateV1::Verified).count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeMode {
    /// Fetch every chunk and verify it against the manifest: the only mode whose `Verified` is a fact.
    Verify,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailabilityV1 {
    pub claim_hex: String,
    pub manifest_id: Hash64,
    pub retention_until_daa: u64,
    pub per_provider: Vec<ProviderAvailabilityV1>,
    /// For each chunk, how many providers served verified bytes.
    pub copies: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvailabilityVerdictV1 {
    /// Every chunk has at least `required` verified copies.
    Healthy { min_copies: usize },
    /// Every chunk has at least one verified copy, some fewer than required: repair while it can be done.
    Degraded { min_copies: usize, thin_chunks: Vec<u32> },
    /// Some chunk has NO verified copy anywhere this machine looked.
    Unavailable { chunks_without_copy: Vec<u32> },
    /// The retention deadline has passed: no obligation to keep it.
    Expired,
}

impl AvailabilityV1 {
    pub fn verdict(&self, required: usize, now_daa: Option<u64>) -> AvailabilityVerdictV1 {
        if now_daa.is_some_and(|now| now > self.retention_until_daa) {
            return AvailabilityVerdictV1::Expired;
        }
        let none: Vec<u32> = self.copies.iter().enumerate().filter(|(_, c)| **c == 0).map(|(i, _)| i as u32).collect();
        if !none.is_empty() {
            return AvailabilityVerdictV1::Unavailable { chunks_without_copy: none };
        }
        let min_copies = self.copies.iter().copied().min().unwrap_or(0);
        if min_copies >= required.max(1) {
            AvailabilityVerdictV1::Healthy { min_copies }
        } else {
            let thin = self.copies.iter().enumerate().filter(|(_, c)| **c < required).map(|(i, _)| i as u32).collect();
            AvailabilityVerdictV1::Degraded { min_copies, thin_chunks: thin }
        }
    }

    /// One line for a person. Always says whose observation it is.
    pub fn line(&self, required: usize, now_daa: Option<u64>) -> String {
        format!(
            "{LOCAL_OBSERVATION}: {:?} — an observation by this machine, not a proof for anyone else and never a slash reason",
            self.verdict(required, now_daa)
        )
    }
}

/// **Look at every provider, chunk by chunk, with bytes** (`ProbeMode::Verify`).
pub fn check_availability_v1(
    claim_hex: &str,
    manifest: &EvidenceManifestV1,
    providers: &[&dyn EvidenceProvider],
    _mode: ProbeMode,
) -> AvailabilityV1 {
    let manifest_id = manifest_id_v1(manifest);
    let mut per_provider = Vec::new();
    let mut copies = vec![0usize; manifest.chunks.len()];
    for p in providers {
        let standing = match p.manifest_for(claim_hex) {
            Ok(Some(m)) if manifest_id_v1(&m) == manifest_id => ManifestStandingV1::Same,
            Ok(Some(m)) => ManifestStandingV1::Different(manifest_id_v1(&m)),
            Ok(None) => ManifestStandingV1::Absent,
            Err(e) => ManifestStandingV1::Unreachable(e.0),
        };
        let mut chunks = Vec::with_capacity(manifest.chunks.len());
        for (i, entry) in manifest.chunks.iter().enumerate() {
            // The chunk bytes are addressed by the manifest id, not by the provider's manifest: a provider may hold chunks without the manifest.
            let state = match p.fetch_chunk(manifest_id, entry.index) {
                Ok(bytes) => match manifest.verify_chunk(entry.index, &bytes) {
                    Ok(()) => {
                        copies[i] += 1;
                        ChunkStateV1::Verified
                    }
                    Err(_) => ChunkStateV1::Corrupt,
                },
                Err(e) if is_absence(&e.0) => ChunkStateV1::Missing,
                Err(e) => ChunkStateV1::Unreachable(e.0),
            };
            chunks.push(state);
        }
        per_provider.push(ProviderAvailabilityV1 { provider: p.provider_id().to_string(), manifest: standing, chunks });
    }
    AvailabilityV1 {
        claim_hex: claim_hex.to_string(),
        manifest_id,
        retention_until_daa: manifest.retention_until_daa,
        per_provider,
        copies,
    }
}

fn is_absence(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("404") || t.contains("no such file") || t.contains("not found") || t.contains("os error 2")
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Repair: anyone holding verified bytes can re-seed a thin provider
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairReportV1 {
    /// `(provider, chunk)` copies made, each read back and verified.
    pub copied: Vec<(String, u32)>,
    /// `(provider, chunk, why)` copies that could not be made.
    pub failed: Vec<(String, u32, String)>,
    /// Chunks no provider could supply a verified copy of — nothing to repair them from.
    pub unrepairable: Vec<u32>,
}

/// **Copy verified chunks to every provider that lacks them.** Sources are every provider's VERIFIED bytes (a corrupt chunk is never copied);
/// the manifest follows on a provider only once all its chunks are there. The miner need not be online: this is what a watcher, a friend or a
/// public verifier does to keep a claim's evidence alive until its retention deadline.
pub fn repair_v1(claim_hex: &str, manifest: &EvidenceManifestV1, providers: &[&dyn EvidenceProvider]) -> RepairReportV1 {
    let manifest_id = manifest_id_v1(manifest);
    let before = check_availability_v1(claim_hex, manifest, providers, ProbeMode::Verify);
    let mut report = RepairReportV1 { copied: Vec::new(), failed: Vec::new(), unrepairable: Vec::new() };
    let refs: Vec<&dyn ChunkProvider> = providers.iter().map(|p| *p as &dyn ChunkProvider).collect();
    for (i, entry) in manifest.chunks.iter().enumerate() {
        let missing_on: Vec<usize> = before
            .per_provider
            .iter()
            .enumerate()
            .filter(|(_, pa)| pa.chunks[i] != ChunkStateV1::Verified && !matches!(pa.chunks[i], ChunkStateV1::Unreachable(_)))
            .map(|(pi, _)| pi)
            .collect();
        if missing_on.is_empty() {
            continue;
        }
        let Ok((bytes, _, _)) = crate::evidence::fetch_chunk_any(manifest, entry.index, &refs, manifest_id) else {
            report.unrepairable.push(entry.index);
            continue;
        };
        for pi in missing_on {
            let p = providers[pi];
            let outcome = p.put_chunk(manifest_id, entry.index, &bytes).map_err(|e| e.0).and_then(|()| {
                let back = p.fetch_chunk(manifest_id, entry.index).map_err(|e| e.0)?;
                manifest.verify_chunk(entry.index, &back).map_err(|e| e.to_string())
            });
            match outcome {
                Ok(()) => report.copied.push((p.provider_id().to_string(), entry.index)),
                Err(why) => report.failed.push((p.provider_id().to_string(), entry.index, why)),
            }
        }
    }
    // The manifest last, and only where every chunk now verifies.
    let after = check_availability_v1(claim_hex, manifest, providers, ProbeMode::Verify);
    for (pi, pa) in after.per_provider.iter().enumerate() {
        if pa.manifest != ManifestStandingV1::Same && pa.verified_chunks() == manifest.chunks.len() {
            let p = providers[pi];
            if let Err(e) = p.put_manifest(claim_hex, manifest) {
                report.failed.push((p.provider_id().to_string(), u32::MAX, format!("manifest: {e}")));
            }
        }
    }
    report
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Retention monitoring after the miner is gone
// ---------------------------------------------------------------------------------------------------------------------------------

/// A claim whose evidence this process keeps alive.
#[derive(Clone, Debug)]
pub struct WatchedClaimV1 {
    pub claim_hex: String,
    pub manifest: EvidenceManifestV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetentionEventV1 {
    Healthy {
        claim_hex: String,
        min_copies: usize,
    },
    /// Thin, and repaired (or an attempt was made).
    Repaired {
        claim_hex: String,
        repair: RepairReportV1,
        after: AvailabilityVerdictV1,
    },
    /// Some chunk has no verified copy anywhere this machine looked and repair had nothing to copy from. Loud. Not a slash reason.
    AtRisk {
        claim_hex: String,
        chunks_without_copy: Vec<u32>,
    },
    /// The retention deadline passed: dropped from the watch list.
    Expired {
        claim_hex: String,
    },
}

pub struct RetentionMonitor {
    pub required_copies: usize,
    pub repair: bool,
    watched: Vec<WatchedClaimV1>,
}

impl RetentionMonitor {
    pub fn new(required_copies: usize, repair: bool) -> Self {
        Self { required_copies: required_copies.max(1), repair, watched: Vec::new() }
    }

    pub fn watch(&mut self, claim: WatchedClaimV1) {
        if !self.watched.iter().any(|w| w.claim_hex == claim.claim_hex) {
            self.watched.push(claim);
        }
    }

    pub fn watching(&self) -> usize {
        self.watched.len()
    }

    /// **One pass** at chain time `now_daa`: look at every watched claim, repair the thin ones, drop the expired ones.
    pub fn tick(&mut self, now_daa: u64, providers: &[&dyn EvidenceProvider]) -> Vec<RetentionEventV1> {
        let mut events = Vec::new();
        let mut keep = Vec::new();
        for w in std::mem::take(&mut self.watched) {
            if now_daa > w.manifest.retention_until_daa {
                events.push(RetentionEventV1::Expired { claim_hex: w.claim_hex });
                continue;
            }
            let a = check_availability_v1(&w.claim_hex, &w.manifest, providers, ProbeMode::Verify);
            match a.verdict(self.required_copies, Some(now_daa)) {
                AvailabilityVerdictV1::Healthy { min_copies } => {
                    events.push(RetentionEventV1::Healthy { claim_hex: w.claim_hex.clone(), min_copies })
                }
                AvailabilityVerdictV1::Expired => events.push(RetentionEventV1::Expired { claim_hex: w.claim_hex.clone() }),
                AvailabilityVerdictV1::Unavailable { chunks_without_copy } => {
                    events.push(RetentionEventV1::AtRisk { claim_hex: w.claim_hex.clone(), chunks_without_copy });
                }
                AvailabilityVerdictV1::Degraded { .. } if self.repair => {
                    let repair = repair_v1(&w.claim_hex, &w.manifest, providers);
                    let after = check_availability_v1(&w.claim_hex, &w.manifest, providers, ProbeMode::Verify)
                        .verdict(self.required_copies, Some(now_daa));
                    events.push(RetentionEventV1::Repaired { claim_hex: w.claim_hex.clone(), repair, after });
                }
                AvailabilityVerdictV1::Degraded { min_copies, .. } => {
                    events.push(RetentionEventV1::Healthy { claim_hex: w.claim_hex.clone(), min_copies })
                }
            }
            keep.push(w);
        }
        self.watched = keep;
        events
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// A reference provider: a directory over HTTP that verifies what it is given
// ---------------------------------------------------------------------------------------------------------------------------------

pub mod server {
    use super::*;

    #[derive(Clone, Debug)]
    pub struct ServerConfig {
        pub limits: ManifestLimits,
        /// Refuse PUTs once the root holds this many bytes (a provider is somebody's disk).
        pub max_store_bytes: u64,
    }

    impl Default for ServerConfig {
        fn default() -> Self {
            Self { limits: ManifestLimits::default(), max_store_bytes: 64 << 30 }
        }
    }

    pub struct ServerHandle {
        pub addr: SocketAddr,
        stop: Arc<AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl ServerHandle {
        pub fn url(&self) -> String {
            format!("http://{}", self.addr)
        }
        pub fn stop(mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    impl Drop for ServerHandle {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    /// Serve `root` on `listen` until the handle is stopped.
    pub fn start(listen: &str, root: PathBuf, cfg: ServerConfig) -> std::io::Result<ServerHandle> {
        let listener = TcpListener::bind(listen)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (root, cfg) = (root.clone(), cfg.clone());
                        std::thread::spawn(move || {
                            let _ = stream.set_nonblocking(false);
                            let _ = handle(stream, &root, &cfg);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => break,
                }
            }
        });
        Ok(ServerHandle { addr, stop, thread: Some(thread) })
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Target {
        Manifest(String),
        Chunk(Hash64, u32),
    }

    fn is_hex128(s: &str) -> bool {
        s.len() == 128 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    /// Strict: nothing but the two content-addressed shapes ever reaches the filesystem (no `..`, no extra segments, no odd names).
    fn parse_target(path: &str) -> Option<Target> {
        let path = path.split('?').next().unwrap_or("");
        if let Some(rest) = path.strip_prefix("/claims/") {
            let claim = rest.strip_suffix(".manifest")?;
            return is_hex128(claim).then(|| Target::Manifest(claim.to_ascii_lowercase()));
        }
        let rest = path.strip_prefix("/chunks/")?;
        let (id, file) = rest.split_once('/')?;
        let index = file.strip_suffix(".chunk")?;
        if !is_hex128(id) || index.is_empty() || index.len() > 10 || !index.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Target::Chunk(id.parse().ok()?, index.parse().ok()?))
    }

    fn respond(stream: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
        let reason = match status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            507 => "Insufficient Storage",
            _ => "Error",
        };
        write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())?;
        stream.write_all(body)
    }

    fn store_size(root: &Path) -> u64 {
        fn walk(dir: &Path) -> u64 {
            std::fs::read_dir(dir)
                .map(|it| {
                    it.flatten()
                        .map(|e| match e.metadata() {
                            Ok(m) if m.is_dir() => walk(&e.path()),
                            Ok(m) => m.len(),
                            Err(_) => 0,
                        })
                        .sum()
                })
                .unwrap_or(0)
        }
        walk(root)
    }

    fn handle(mut stream: TcpStream, root: &Path, cfg: &ServerConfig) -> std::io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        // The head, bounded.
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        let split = loop {
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break i;
            }
            if raw.len() > 16 * 1024 {
                return respond(&mut stream, 400, b"header too large");
            }
            let n = stream.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            raw.extend_from_slice(&buf[..n]);
        };
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let mut lines = head.lines();
        let mut request = lines.next().unwrap_or("").split_whitespace();
        let (method, path) = (request.next().unwrap_or(""), request.next().unwrap_or(""));
        let content_length = lines
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let Some(target) = parse_target(path) else { return respond(&mut stream, 404, b"not a content-addressed path") };
        let file = match &target {
            Target::Manifest(claim) => crate::evidence::fs::manifest_path(root, claim),
            Target::Chunk(id, index) => crate::evidence::fs::chunk_path(root, *id, *index),
        };
        match method {
            "GET" | "HEAD" => match std::fs::read(&file) {
                Ok(bytes) if method == "HEAD" => respond(&mut stream, 200, &[]).map(|_| drop(bytes)),
                Ok(bytes) => respond(&mut stream, 200, &bytes),
                Err(_) => respond(&mut stream, 404, b"not found"),
            },
            "PUT" => {
                let cap = match &target {
                    Target::Manifest(_) => MANIFEST_MAX_BYTES,
                    Target::Chunk(..) => cfg.limits.max_chunk_bytes as usize,
                };
                if content_length > cap {
                    // Drain (a bounded amount of) what the client is still sending before answering: closing on unread data resets the
                    // connection and the client would never see the 413.
                    let mut left = content_length.saturating_sub(raw.len() - split - 4).min(4 << 20);
                    while left > 0 {
                        match stream.read(&mut buf[..left.min(4096)]) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => left -= n,
                        }
                    }
                    return respond(&mut stream, 413, b"too large");
                }
                let mut body = raw[split + 4..].to_vec();
                while body.len() < content_length {
                    let n = stream.read(&mut buf)?;
                    if n == 0 {
                        return respond(&mut stream, 400, b"short body");
                    }
                    body.extend_from_slice(&buf[..n]);
                    if body.len() > cap {
                        return respond(&mut stream, 413, b"too large");
                    }
                }
                body.truncate(content_length);
                if store_size(root).saturating_add(body.len() as u64) > cfg.max_store_bytes {
                    return respond(&mut stream, 507, b"this provider is full");
                }
                match target {
                    Target::Chunk(..) => match write_atomic(&file, &body) {
                        Ok(()) => respond(&mut stream, 201, b"stored"),
                        Err(e) => respond(&mut stream, 400, e.to_string().as_bytes()),
                    },
                    // The manifest is accepted only if it is well formed AND every chunk it names is already here and hashes to its entry: this
                    // provider never serves a manifest it cannot back.
                    Target::Manifest(_) => {
                        let manifest = match decode_manifest(&body) {
                            Ok(m) => m,
                            Err(e) => return respond(&mut stream, 400, e.0.as_bytes()),
                        };
                        if let Err(e) = manifest.validate_shape(&cfg.limits) {
                            return respond(&mut stream, 400, e.to_string().as_bytes());
                        }
                        let id = manifest_id_v1(&manifest);
                        for entry in &manifest.chunks {
                            let stored = std::fs::read(crate::evidence::fs::chunk_path(root, id, entry.index));
                            match stored {
                                Ok(bytes) if manifest.verify_chunk(entry.index, &bytes).is_ok() => {}
                                _ => {
                                    return respond(
                                        &mut stream,
                                        409,
                                        format!("chunk {} is missing or does not match the manifest", entry.index).as_bytes(),
                                    );
                                }
                            }
                        }
                        match write_atomic(&file, &body) {
                            Ok(()) => respond(&mut stream, 201, b"stored"),
                            Err(e) => respond(&mut stream, 400, e.to_string().as_bytes()),
                        }
                    }
                }
            }
            _ => respond(&mut stream, 405, b"GET, HEAD or PUT"),
        }
    }

    #[cfg(test)]
    pub(crate) fn parse_target_for_test(path: &str) -> bool {
        parse_target(path).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{ClaimRoots, fs::FsProvider};
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-transport-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn chunks() -> Vec<Vec<u8>> {
        vec![vec![1u8; 4000], vec![2u8; 2500], vec![3u8; 77]]
    }

    fn manifest() -> EvidenceManifestV1 {
        EvidenceManifestV1::build(h(9), &TransactionOutpoint::new(h(0xB0), 1), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &chunks())
    }

    fn roots() -> ClaimRoots {
        ClaimRoots {
            network_domain: h(9),
            trace_root: h(1),
            output_root: h(2),
            execution_root: h(3),
            trace_chunk_count: 3,
            retention_deadline: 9_000,
        }
    }

    const CLAIM: &str = "ab";

    fn claim_hex() -> String {
        CLAIM.repeat(64)
    }

    fn up(name: &str) -> (server::ServerHandle, PathBuf, HttpProvider) {
        let dir = scratch(name);
        let handle = server::start("127.0.0.1:0", dir.clone(), server::ServerConfig::default()).unwrap();
        let provider = HttpProvider::new(handle.url());
        (handle, dir, provider)
    }

    #[test]
    fn a_provider_list_parses_dedups_and_refuses_unknown_schemes() {
        let list = "# my providers\n/data/a\n dir:/data/b , http://127.0.0.1:9000/ \nhttps://example.test/prefix/\n/data/a\n";
        let specs = parse_provider_list_v1(list).unwrap();
        assert_eq!(
            specs,
            vec![
                ProviderSpecV1::Dir("/data/a".into()),
                ProviderSpecV1::Dir("/data/b".into()),
                ProviderSpecV1::Http("http://127.0.0.1:9000".into()),
                ProviderSpecV1::Http("https://example.test/prefix".into()),
            ]
        );
        assert!(parse_provider_list_v1("ftp://x/y").is_err());
        assert!(parse_provider_list_v1("\n# nothing\n").unwrap().is_empty());
    }

    #[test]
    fn three_providers_hold_the_evidence_and_the_node_the_panel_and_a_public_verifier_read_the_same_bytes() {
        let (h1, d1, p1) = up("a");
        let (h2, _d2, p2) = up("b");
        let dir3 = scratch("c");
        let p3 = FsProvider::new(dir3.clone());
        let manifest = manifest();
        let providers: Vec<&dyn EvidenceProvider> = vec![&p1, &p2, &p3];
        let report =
            publish_to_providers_v1(&claim_hex(), &manifest, &chunks(), &providers, 3).expect("all three hold a verified copy");
        assert_eq!(report.verified_copies(), 3);

        // The public verifier: HTTP providers only, the CLAIM's roots from the chain.
        let http: Vec<&dyn EvidenceProvider> = vec![&p1, &p2];
        let (public_bytes, public_report) =
            fetch_claim_material_any(&http, &claim_hex(), &roots(), &ManifestLimits::default(), h(7)).unwrap();
        assert_eq!(public_bytes, chunks().concat());
        assert_eq!(public_report.served_by.len(), 3);
        // The node: the directory flag, through the function kaspad already calls.
        let (node_bytes, _) = crate::evidence::fs::fetch_claim_material(
            std::slice::from_ref(&dir3),
            &claim_hex(),
            &roots(),
            &ManifestLimits::default(),
            h(7),
        )
        .unwrap();
        assert_eq!(node_bytes, public_bytes, "the same bytes, whoever asks and whatever carries them");
        // The Panel: any mix of both.
        let mixed: Vec<&dyn EvidenceProvider> = vec![&p3, &p2];
        let (panel_bytes, _) = fetch_claim_material_any(&mixed, &claim_hex(), &roots(), &ManifestLimits::default(), h(7)).unwrap();
        assert_eq!(panel_bytes, public_bytes);

        // One provider disappears, another's disk corrupts a chunk: the evidence is still fetched, and the liar is recorded not believed.
        h2.stop();
        let id = manifest_id_v1(&manifest);
        let victim = crate::evidence::fs::chunk_path(&d1, id, 1);
        let mut bytes = std::fs::read(&victim).unwrap();
        bytes[0] ^= 0xFF;
        std::fs::write(&victim, bytes).unwrap();
        // Without the third copy the corrupt chunk cannot be read at all: the liar is recorded, the dead one too, and nothing is accepted.
        let two_bad: Vec<&dyn EvidenceProvider> = vec![&p1, &p2];
        match fetch_claim_material_any(&two_bad, &claim_hex(), &roots(), &ManifestLimits::default(), h(7)) {
            Err(ClaimFetchErrorV1::Fetch(crate::evidence::FetchFailure::ChunkUnavailable { index: 1, failures })) => {
                assert!(
                    failures.iter().any(|f| matches!(f.kind, crate::evidence::FetchFailureKind::BadBytes(_))),
                    "the corrupt chunk was recorded, not accepted"
                );
                assert!(
                    failures.iter().any(|f| matches!(f.kind, crate::evidence::FetchFailureKind::Unavailable(_))),
                    "the dead provider was recorded"
                );
            }
            other => panic!("{other:?}"),
        }
        // With the directory copy the same evidence is fetched, whatever order the providers are tried in.
        let (again, _) =
            fetch_claim_material_any(&mixed_all(&p1, &p2, &p3), &claim_hex(), &roots(), &ManifestLimits::default(), h(7)).unwrap();
        assert_eq!(again, public_bytes);

        // A manifest that disagrees with the CLAIM's roots is another execution's evidence: refused wherever it came from.
        let mut other = roots();
        other.execution_root = h(0x44);
        assert!(matches!(
            fetch_claim_material_any(&mixed_all(&p1, &p2, &p3), &claim_hex(), &other, &ManifestLimits::default(), h(7)),
            Err(ClaimFetchErrorV1::NoAdmissibleManifest { .. })
        ));
        h1.stop();
    }

    fn mixed_all<'a>(a: &'a HttpProvider, b: &'a HttpProvider, c: &'a FsProvider) -> Vec<&'a dyn EvidenceProvider> {
        vec![a, b, c]
    }

    #[test]
    fn a_reference_provider_refuses_what_it_cannot_back() {
        let (handle, dir, p) = up("refuse");
        let m = manifest();
        let id = manifest_id_v1(&m);
        // Only the two content-addressed shapes reach the disk.
        for bad in
            ["/../etc/passwd", "/claims/../x.manifest", "/chunks/zz/0.chunk", "/chunks/x", "/claims/ab.manifest", "/chunks//0.chunk"]
        {
            assert!(!server::parse_target_for_test(bad), "{bad}");
        }
        assert!(server::parse_target_for_test(&format!("/chunks/{id}/12.chunk")));
        // A manifest before its chunks: refused (409), and nothing is served for the claim.
        assert!(p.put_manifest(&claim_hex(), &m).unwrap_err().0.contains("409"));
        assert_eq!(p.manifest_for(&claim_hex()).unwrap(), None);
        // A chunk of the wrong bytes: accepted as bytes, but the manifest cannot be backed by it.
        p.put_chunk(id, 0, &chunks()[0]).unwrap();
        p.put_chunk(id, 1, &[0u8; 2500]).unwrap();
        p.put_chunk(id, 2, &chunks()[2]).unwrap();
        assert!(p.put_manifest(&claim_hex(), &m).unwrap_err().0.contains("409"), "chunk 1 does not hash to the manifest");
        p.put_chunk(id, 1, &chunks()[1]).unwrap();
        p.put_manifest(&claim_hex(), &m).unwrap();
        assert_eq!(p.manifest_for(&claim_hex()).unwrap(), Some(m.clone()));
        // An oversized chunk, and a manifest that is not a manifest.
        assert!(p.put_chunk(id, 3, &vec![0u8; (1 << 20) + 1]).unwrap_err().0.contains("413"));
        let http = StdHttp::default();
        let junk = http
            .request("PUT", &format!("{}/claims/{}.manifest", handle.url(), "cd".repeat(64)), Some(b"not a manifest"), 4096)
            .unwrap();
        assert_eq!(junk.status, 400);
        assert_eq!(
            http.request("DELETE", &format!("{}/claims/{}.manifest", handle.url(), claim_hex()), None, 4096).unwrap().status,
            405
        );
        assert!(!dir.join("claims").join(format!("{}.manifest", "cd".repeat(64))).exists());
        handle.stop();
    }

    #[test]
    fn availability_is_a_local_observation_and_distinguishes_missing_corrupt_and_unreachable() {
        let (ha, da, pa) = up("avail-a");
        let (hb, _db, pb) = up("avail-b");
        let (_hc, _dc, pc) = up("avail-c");
        let m = manifest();
        let id = manifest_id_v1(&m);
        let all: Vec<&dyn EvidenceProvider> = vec![&pa, &pb, &pc];
        publish_to_providers_v1(&claim_hex(), &m, &chunks(), &all, 3).unwrap();
        let a = check_availability_v1(&claim_hex(), &m, &all, ProbeMode::Verify);
        assert_eq!(a.copies, vec![3, 3, 3]);
        assert_eq!(a.verdict(3, Some(5_000)), AvailabilityVerdictV1::Healthy { min_copies: 3 });
        assert!(a.line(3, Some(5_000)).starts_with(LOCAL_OBSERVATION));
        // A deleted chunk, a corrupted chunk, a dead provider.
        std::fs::remove_file(crate::evidence::fs::chunk_path(&da, id, 0)).unwrap();
        let mut c1 = std::fs::read(crate::evidence::fs::chunk_path(&da, id, 1)).unwrap();
        c1[3] ^= 1;
        std::fs::write(crate::evidence::fs::chunk_path(&da, id, 1), c1).unwrap();
        hb.stop();
        let a = check_availability_v1(&claim_hex(), &m, &all, ProbeMode::Verify);
        let on_a = &a.per_provider[0];
        assert_eq!(
            (&on_a.chunks[0], &on_a.chunks[1], &on_a.chunks[2]),
            (&ChunkStateV1::Missing, &ChunkStateV1::Corrupt, &ChunkStateV1::Verified)
        );
        assert!(
            a.per_provider[1].chunks.iter().all(|c| matches!(c, ChunkStateV1::Unreachable(_))),
            "a dead provider is silence, not corruption"
        );
        assert_eq!(a.copies, vec![1, 1, 2]);
        assert_eq!(a.verdict(2, Some(5_000)), AvailabilityVerdictV1::Degraded { min_copies: 1, thin_chunks: vec![0, 1] });
        // Past the retention deadline there is no obligation left.
        assert_eq!(a.verdict(2, Some(10_001)), AvailabilityVerdictV1::Expired);
        // No copy anywhere: Unavailable names the chunks (and still no slash — it is an observation).
        let empty = scratch("avail-empty");
        let none = FsProvider::new(empty);
        let only: Vec<&dyn EvidenceProvider> = vec![&none];
        assert_eq!(
            check_availability_v1(&claim_hex(), &m, &only, ProbeMode::Verify).verdict(1, None),
            AvailabilityVerdictV1::Unavailable { chunks_without_copy: vec![0, 1, 2] }
        );
        // A provider holding a DIFFERENT manifest under this claim id is recorded, not used.
        let mut conflicting = m.clone();
        conflicting.retention_until_daa += 1;
        let other = FsProvider::new(scratch("avail-other"));
        other.put_manifest(&claim_hex(), &conflicting).unwrap();
        let a = check_availability_v1(&claim_hex(), &m, &[&other as &dyn EvidenceProvider], ProbeMode::Verify);
        assert!(matches!(a.per_provider[0].manifest, ManifestStandingV1::Different(_)));
        ha.stop();
    }

    #[test]
    fn a_watcher_repairs_a_thin_provider_from_verified_copies_after_the_miner_is_gone() {
        let (ha, da, pa) = up("repair-a");
        let (_hb, db, pb) = up("repair-b");
        let fresh = FsProvider::new(scratch("repair-new"));
        let m = manifest();
        let id = manifest_id_v1(&m);
        let two: Vec<&dyn EvidenceProvider> = vec![&pa, &pb];
        publish_to_providers_v1(&claim_hex(), &m, &chunks(), &two, 2).unwrap();
        // The miner is gone. A: chunk 1 corrupted. B: chunk 2 deleted. A new provider joined and holds nothing.
        let mut c = std::fs::read(crate::evidence::fs::chunk_path(&da, id, 1)).unwrap();
        c[0] ^= 0xFF;
        std::fs::write(crate::evidence::fs::chunk_path(&da, id, 1), c).unwrap();
        std::fs::remove_file(crate::evidence::fs::chunk_path(&db, id, 2)).unwrap();
        let all: Vec<&dyn EvidenceProvider> = vec![&pa, &pb, &fresh];
        let mut monitor = RetentionMonitor::new(3, true);
        monitor.watch(WatchedClaimV1 { claim_hex: claim_hex(), manifest: m.clone() });
        monitor.watch(WatchedClaimV1 { claim_hex: claim_hex(), manifest: m.clone() });
        assert_eq!(monitor.watching(), 1, "a claim is watched once");
        let events = monitor.tick(5_000, &all);
        match &events[0] {
            RetentionEventV1::Repaired { repair, after, .. } => {
                assert!(repair.unrepairable.is_empty() && repair.failed.is_empty(), "{repair:?}");
                assert!(!repair.copied.is_empty());
                assert_eq!(
                    *after,
                    AvailabilityVerdictV1::Healthy { min_copies: 3 },
                    "every provider holds every chunk, verified, after the pass"
                );
            }
            other => panic!("{other:?}"),
        }
        // The corrupt chunk was never copied FROM A: B's good bytes replaced it.
        let a = check_availability_v1(&claim_hex(), &m, &all, ProbeMode::Verify);
        assert_eq!(a.copies, vec![3, 3, 3]);
        assert!(
            EvidenceProvider::manifest_for(&fresh, &claim_hex()).unwrap().is_some(),
            "the manifest followed the chunks on the new provider"
        );
        // Next pass: healthy. Past the deadline: dropped.
        assert!(matches!(monitor.tick(5_100, &all)[0], RetentionEventV1::Healthy { min_copies: 3, .. }));
        assert!(matches!(monitor.tick(10_001, &all)[0], RetentionEventV1::Expired { .. }));
        assert_eq!(monitor.watching(), 0);
        // With NO verified copy left anywhere, repair has nothing to copy: loud, and no pretence.
        ha.stop();
        let lone = FsProvider::new(scratch("repair-none"));
        let only: Vec<&dyn EvidenceProvider> = vec![&lone];
        let mut monitor = RetentionMonitor::new(1, true);
        monitor.watch(WatchedClaimV1 { claim_hex: claim_hex(), manifest: m });
        assert!(
            matches!(&monitor.tick(5_000, &only)[0], RetentionEventV1::AtRisk { chunks_without_copy, .. } if chunks_without_copy == &vec![0, 1, 2])
        );
    }

    #[test]
    fn a_response_that_chunks_its_body_or_lies_about_its_size_is_handled_or_refused() {
        let body = b"hello";
        let chunked = format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n", body.len(), "hello");
        let r = parse_response(chunked.as_bytes(), 100).unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &body[..]));
        // A body above the cap the caller asked for is refused: a provider cannot make a client buffer more.
        assert!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\n123456789", 4).is_err());
        assert!(parse_response(b"garbage", 100).is_err());
        // https is not spoken by the std transport.
        assert!(StdHttp::default().request("GET", "https://example.test/x", None, 10).unwrap_err().contains("http://"));
    }
}
