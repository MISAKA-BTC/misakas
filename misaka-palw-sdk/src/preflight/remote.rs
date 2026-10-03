//! **A repository read by HTTP ranges** (RFC-0002 Part II §II.2.1, "a model repository id"): `config.json`, the index and the 8-byte
//! length and JSON header of each safetensors shard — kilobytes of a repository of hundreds of gigabytes — and nothing else. The
//! headers are written into a scratch directory shaped like a local snapshot (the shards as **sparse** files of their real length: the
//! sizes the report states are the repository's, and no data region was ever fetched), and the one local preflight reads that, so a
//! repository and its directory give the same report but for the input's kind and label.
//!
//! * **No network in the library.** The transport is [`RangeFetcher`] (the lowerer's trait): [`HttpRangeFetcher`] speaks plain
//!   `http://` over a socket (a mirror, a cache, the loopback fixture server the tests run); `https://` goes through the lowerer's
//!   `curl` fetcher (cargo feature `remote`), and nothing here runs unless a caller constructs it. No test uses the hub.
//! * **A base URL names the repository**: `<base>/config.json` resolves it; the Hugging Face spelling is
//!   `https://huggingface.co/<org>/<name>/resolve/<revision>` ([`hf_base_url`]).
//! * **What is fetched**, in full: `config.json`, `model.safetensors.index.json`, the shard headers. What is only measured (a length
//!   probe, a sparse file of that length): the tokenizer and preprocessor files and the files the checkpoint's neighbours are known by.

use misaka_palw_tir_lower::LowerError;
use misaka_palw_tir_lower::weights::RangeFetcher;
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Files beside a checkpoint whose presence and size the preflight reports (§II.2.1): measured, never read.
pub const MEASURED_FILES_V1: &[&str] = &[
    "generation_config.json",
    "preprocessor_config.json",
    "tokenizer.json",
    "tokenizer.model",
    "tokenizer_config.json",
    "vocab.json",
    "merges.txt",
    "special_tokens_map.json",
    "added_tokens.json",
    "chat_template.json",
];

/// The base URL of a Hugging Face repository id at a revision on an endpoint (`https://huggingface.co` unless overridden).
pub fn hf_base_url(endpoint: &str, repo: &str, revision: &str) -> String {
    format!("{}/{}/resolve/{}", endpoint.trim_end_matches('/'), repo.trim_matches('/'), revision)
}

/// A plain `http://` range client over a socket: `HEAD`-style length probes by a one-byte range, ranges by `Range: bytes=a-b`.
pub struct HttpRangeFetcher {
    timeout: std::time::Duration,
}

impl Default for HttpRangeFetcher {
    fn default() -> Self {
        Self { timeout: std::time::Duration::from_secs(30) }
    }
}

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn split_url(url: &str) -> Result<(String, String), LowerError> {
    let rest = url.strip_prefix("http://").ok_or_else(|| LowerError::Io(format!("{url}: this client speaks http:// (https goes through curl, cargo feature `remote`)")))?;
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h.to_string(), format!("/{p}")),
        None => (rest.to_string(), "/".to_string()),
    };
    Ok((host, path))
}

impl HttpRangeFetcher {
    fn get(&self, url: &str, range: Option<Range<u64>>) -> Result<Response, LowerError> {
        let (host, path) = split_url(url)?;
        let mut stream = TcpStream::connect(&host).map_err(|e| LowerError::Io(format!("{url}: {e}")))?;
        stream.set_read_timeout(Some(self.timeout)).ok();
        stream.set_write_timeout(Some(self.timeout)).ok();
        let mut req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: */*\r\n");
        if let Some(r) = &range {
            req.push_str(&format!("Range: bytes={}-{}\r\n", r.start, r.end.saturating_sub(1)));
        }
        req.push_str("\r\n");
        stream.write_all(req.as_bytes()).map_err(|e| LowerError::Io(format!("{url}: {e}")))?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| LowerError::Io(format!("{url}: {e}")))?;
        let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| LowerError::Io(format!("{url}: no HTTP response")))?;
        let head = String::from_utf8_lossy(&raw[..cut]).to_string();
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or_else(|| LowerError::Io(format!("{url}: no HTTP status")))?;
        let headers = lines.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
        Ok(Response { status, headers, body: raw[cut + 4..].to_vec() })
    }
}

impl RangeFetcher for HttpRangeFetcher {
    fn len(&self, url: &str) -> Result<Option<u64>, LowerError> {
        let r = self.get(url, Some(0..1))?;
        match r.status {
            404 => Ok(None),
            206 => r
                .header("content-range")
                .and_then(|v| v.rsplit('/').next())
                .and_then(|n| n.trim().parse::<u64>().ok())
                .map(Some)
                .ok_or_else(|| LowerError::Io(format!("{url}: 206 without a Content-Range length"))),
            // A server that ignores ranges answers 200 with the whole body.
            200 => Ok(Some(r.body.len() as u64)),
            s => Err(LowerError::Io(format!("{url}: HTTP {s}"))),
        }
    }

    fn fetch(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>, LowerError> {
        if range.is_empty() {
            return Ok(Vec::new());
        }
        let r = self.get(url, Some(range.clone()))?;
        match r.status {
            206 => {
                if r.body.len() as u64 != range.end - range.start {
                    return Err(LowerError::Io(format!("{url}: asked {range:?}, the server sent {} bytes", r.body.len())));
                }
                Ok(r.body)
            }
            s => Err(LowerError::Io(format!("{url}: HTTP {s} (a server that honours Range answers 206)"))),
        }
    }
}

/// What a remote read did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteStats {
    /// Bytes fetched (the configuration, the index, the headers).
    pub fetched_bytes: u64,
    /// Requests made.
    pub requests: u64,
}

struct Counting<'a> {
    inner: &'a dyn RangeFetcher,
    bytes: std::sync::atomic::AtomicU64,
    requests: std::sync::atomic::AtomicU64,
}

impl RangeFetcher for Counting<'_> {
    fn len(&self, url: &str) -> Result<Option<u64>, LowerError> {
        self.requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.len(url)
    }
    fn fetch(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>, LowerError> {
        let b = self.inner.fetch(url, range)?;
        self.requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.bytes.fetch_add(b.len() as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(b)
    }
}

fn write_sparse(dir: &Path, name: &str, head: &[u8], len: u64) -> Result<(), String> {
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).map_err(|e| format!("{name}: {e}"))?;
    f.write_all(head).map_err(|e| format!("{name}: {e}"))?;
    if len > head.len() as u64 {
        f.seek(SeekFrom::Start(len - 1)).map_err(|e| format!("{name}: {e}"))?;
        f.write_all(&[0]).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}

/// **Fetch what a preflight reads of the repository at `base` into `dir`**: `config.json`, the index, each shard's header (the 8-byte
/// length and the JSON), and the length of every measured file. Returns what was fetched.
pub fn materialize_headers(fetcher: &dyn RangeFetcher, base: &str, dir: &Path) -> Result<RemoteStats, String> {
    let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
    let counting = Counting { inner: fetcher, bytes: Default::default(), requests: Default::default() };
    let get_whole = |name: &str| -> Result<Option<Vec<u8>>, String> {
        let url = format!("{base}{name}");
        match counting.len(&url).map_err(|e| e.to_string())? {
            None => Ok(None),
            Some(n) => counting.fetch(&url, 0..n).map(Some).map_err(|e| e.to_string()),
        }
    };
    let config = get_whole("config.json")?.ok_or_else(|| format!("{base}config.json: not found (is this a model repository?)"))?;
    std::fs::write(dir.join("config.json"), &config).map_err(|e| e.to_string())?;
    // The shards: the index names them; without one, a lone model.safetensors.
    let mut shard_names: BTreeSet<String> = BTreeSet::new();
    if let Some(index) = get_whole("model.safetensors.index.json")? {
        std::fs::write(dir.join("model.safetensors.index.json"), &index).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_slice(&index).map_err(|e| format!("model.safetensors.index.json: {e}"))?;
        let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or("model.safetensors.index.json has no weight_map")?;
        shard_names.extend(wm.values().filter_map(|x| x.as_str().map(str::to_string)));
    } else if counting.len(&format!("{base}model.safetensors")).map_err(|e| e.to_string())?.is_some() {
        shard_names.insert("model.safetensors".into());
    }
    for name in &shard_names {
        if name.contains('/') || name.contains("..") {
            return Err(format!("the index names a shard `{name}` that is not a file beside it"));
        }
        let url = format!("{base}{name}");
        let len = counting.len(&url).map_err(|e| e.to_string())?.ok_or_else(|| format!("{name}: named by the index and not found"))?;
        if len < 8 {
            return Err(format!("{name}: shorter than the 8-byte header length"));
        }
        let head = counting.fetch(&url, 0..8).map_err(|e| e.to_string())?;
        let n = u64::from_le_bytes(head[..8].try_into().expect("8 bytes"));
        if n == 0 || n > (256 << 20) || 8 + n > len {
            return Err(format!("{name}: a header of {n} bytes is not a safetensors header"));
        }
        let mut bytes = head;
        bytes.extend(counting.fetch(&url, 8..8 + n).map_err(|e| e.to_string())?);
        write_sparse(dir, name, &bytes, len)?;
    }
    for name in MEASURED_FILES_V1 {
        if let Some(len) = counting.len(&format!("{base}{name}")).map_err(|e| e.to_string())? {
            write_sparse(dir, name, &[], len)?;
        }
    }
    Ok(RemoteStats {
        fetched_bytes: counting.bytes.load(std::sync::atomic::Ordering::Relaxed),
        requests: counting.requests.load(std::sync::atomic::Ordering::Relaxed),
    })
}

/// A scratch directory of its own, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Result<Scratch, String> {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "palw-preflight-remote-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        Ok(Scratch(d))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
