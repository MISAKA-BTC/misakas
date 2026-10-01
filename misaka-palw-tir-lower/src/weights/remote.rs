//! **A checkpoint read by range fetches** — the same `TensorSource` as a local shard directory,
//! over any [`RangeFetcher`] (an HTTP server that honours `Range`, a mirror, an object store).
//!
//! * **Header only, until data is asked for.** [`RemoteCheckpoint::open`] fetches the shard index
//!   (when the repository has one) and, of each shard, the 8-byte length and the JSON header —
//!   kilobytes of a repository of hundreds of gigabytes. [`TensorSource::metadata`] answers from
//!   them, which is all the preflight needs ("what is in this model, in what types, how big").
//! * **Range reads for data.** [`TensorSource::read_slice`] and
//!   [`load_rows`](TensorSource::load_rows) fetch exactly the bytes asked for, so a conversion can
//!   stream a model it never stores whole.
//! * **No network in the library.** The transport is the trait. The only one that touches a
//!   network, [`CurlFetcher`] (behind the `remote` cargo feature, off by default), runs the
//!   system's `curl`; every test uses [`MemoryFetcher`].

use super::{Entry, Tensor, TensorMeta, TensorSource, parse_header, widen_floats};
use crate::error::{LowerError, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

/// Serves byte ranges of named resources (URLs).
pub trait RangeFetcher: Send + Sync {
    /// The resource's length; `Ok(None)` when it does not exist (a repository without a shard
    /// index), an error when it could not be asked.
    fn len(&self, url: &str) -> Result<Option<u64>>;
    /// Bytes `range` of the resource (exactly `range.len()` of them, or an error).
    fn fetch(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>>;
}

/// Resources held in memory: for tests, and for preflighting data that is already in hand
/// (a header file, a synthetic config). Counts what was asked of it.
#[derive(Default)]
pub struct MemoryFetcher {
    pub files: BTreeMap<String, Vec<u8>>,
    requests: AtomicU64,
    bytes: AtomicU64,
}

impl MemoryFetcher {
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> Self {
        Self { files, ..Default::default() }
    }
    /// Requests served, and bytes handed out, since creation.
    pub fn served(&self) -> (u64, u64) {
        (self.requests.load(Ordering::Relaxed), self.bytes.load(Ordering::Relaxed))
    }
}

impl RangeFetcher for MemoryFetcher {
    fn len(&self, url: &str) -> Result<Option<u64>> {
        Ok(self.files.get(url).map(|b| b.len() as u64))
    }
    fn fetch(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let b = self.files.get(url).ok_or_else(|| LowerError::Io(format!("{url}: not found")))?;
        if range.start > range.end || range.end > b.len() as u64 {
            return Err(LowerError::Io(format!("{url}: bytes {range:?} of {}", b.len())));
        }
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(range.end - range.start, Ordering::Relaxed);
        Ok(b[range.start as usize..range.end as usize].to_vec())
    }
}

struct Shard {
    url: String,
    entries: BTreeMap<String, Entry>,
    data_offset: u64,
}

/// A safetensors repository (single file or sharded) read through a [`RangeFetcher`].
pub struct RemoteCheckpoint {
    fetcher: Box<dyn RangeFetcher>,
    shards: Vec<Shard>,
    index: BTreeMap<String, usize>,
    fetched: AtomicU64,
}

/// A header larger than this is refused before it is fetched (the local reader's cap).
const MAX_HEADER: u64 = 100 * 1024 * 1024;

impl RemoteCheckpoint {
    /// Open the repository at `base` (a URL prefix; `file` is `base + file`): its
    /// `model.safetensors.index.json` and the headers of the shards it names, or a lone
    /// `model.safetensors`.
    pub fn open(fetcher: Box<dyn RangeFetcher>, base: &str) -> Result<Self> {
        let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
        let index_url = format!("{base}model.safetensors.index.json");
        let mut me = RemoteCheckpoint { fetcher, shards: Vec::new(), index: BTreeMap::new(), fetched: AtomicU64::new(0) };
        let urls: Vec<String> = match me.fetcher.len(&index_url)? {
            Some(n) => {
                let bytes = me.get(&index_url, 0..n)?;
                let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| LowerError::weights(format!("index: {e}")))?;
                let wm = v.get("weight_map").and_then(|w| w.as_object()).ok_or_else(|| LowerError::weights("index has no weight_map"))?;
                let shards: BTreeSet<&str> = wm.values().filter_map(|x| x.as_str()).collect();
                let urls: Vec<String> = shards.iter().map(|s| format!("{base}{s}")).collect();
                for (name, shard) in wm {
                    if !shards.contains(shard.as_str().unwrap_or("")) {
                        return Err(LowerError::weights(format!("index names `{name}` in a shard that is not a string")));
                    }
                }
                urls
            }
            None => vec![format!("{base}model.safetensors")],
        };
        for url in urls {
            let len = me.fetcher.len(&url)?.ok_or_else(|| LowerError::Io(format!("{url}: not found")))?;
            if len < 8 {
                return Err(LowerError::weights(format!("{url}: shorter than the 8-byte header length")));
            }
            let head = me.get(&url, 0..8)?;
            let n = u64::from_le_bytes(head[..8].try_into().map_err(|_| LowerError::weights("header length"))?);
            if n > MAX_HEADER || 8 + n > len {
                return Err(LowerError::weights(format!("{url}: implausible header length {n}")));
            }
            let mut buf = head;
            buf.extend(me.get(&url, 8..8 + n)?);
            let (entries, data_offset) = parse_header(&buf, len)?;
            let si = me.shards.len();
            for name in entries.keys() {
                if me.index.insert(name.clone(), si).is_some() {
                    return Err(LowerError::weights(format!("tensor `{name}` appears in two shards")));
                }
            }
            me.shards.push(Shard { url, entries, data_offset });
        }
        Ok(me)
    }

    fn get(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let b = self.fetcher.fetch(url, range.clone())?;
        if b.len() as u64 != range.end - range.start {
            return Err(LowerError::Io(format!("{url}: asked {range:?}, got {} bytes", b.len())));
        }
        self.fetched.fetch_add(b.len() as u64, Ordering::Relaxed);
        Ok(b)
    }

    /// Bytes fetched since opening (headers included).
    pub fn fetched_bytes(&self) -> u64 {
        self.fetched.load(Ordering::Relaxed)
    }

    fn at(&self, name: &str) -> Result<(&Shard, &Entry)> {
        let i = self.index.get(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        let s = &self.shards[*i];
        Ok((s, &s.entries[name]))
    }
}

impl TensorSource for RemoteCheckpoint {
    fn shape(&self, name: &str) -> Option<Vec<usize>> {
        self.at(name).ok().map(|(_, e)| e.shape.clone())
    }
    fn names(&self) -> Vec<String> {
        self.index.keys().cloned().collect()
    }
    fn load(&self, name: &str) -> Result<Tensor> {
        let (_, e) = self.at(name)?;
        let raw = self.read_slice(name, 0..e.end - e.begin)?;
        Ok(Tensor::new(e.shape.clone(), widen_floats(&e.dtype, &raw)?))
    }
    fn load_i32(&self, name: &str) -> Result<(Vec<usize>, Vec<i32>)> {
        let (_, e) = self.at(name)?;
        if e.dtype != "I32" {
            return Err(LowerError::weights(format!("`{name}` is {}, not I32", e.dtype)));
        }
        let raw = self.read_slice(name, 0..e.end - e.begin)?;
        Ok((e.shape.clone(), raw.chunks_exact(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()))
    }
    fn metadata(&self, name: &str) -> Option<TensorMeta> {
        self.at(name).ok().map(|(_, e)| TensorMeta { dtype: e.dtype.clone(), shape: e.shape.clone(), bytes: e.end - e.begin })
    }
    fn read_slice(&self, name: &str, range: Range<u64>) -> Result<Vec<u8>> {
        let (s, e) = self.at(name)?;
        if range.start > range.end || range.end > e.end - e.begin {
            return Err(LowerError::weights(format!("`{name}`: bytes {range:?} of a tensor of {}", e.end - e.begin)));
        }
        let at = s.data_offset + e.begin;
        self.get(&s.url, at + range.start..at + range.end)
    }
    fn load_rows(&self, name: &str, rows: Range<usize>) -> Result<Tensor> {
        let meta = self.metadata(name).ok_or_else(|| LowerError::weights(format!("no tensor `{name}`")))?;
        let (n, cols) = meta.rows_cols();
        if rows.start > rows.end || rows.end > n {
            return Err(LowerError::weights(format!("`{name}`: rows {rows:?} of {n}")));
        }
        let rb = meta.row_bytes();
        let raw = self.read_slice(name, rows.start as u64 * rb..rows.end as u64 * rb)?;
        Ok(Tensor::new(vec![rows.len(), cols], widen_floats(&meta.dtype, &raw)?))
    }
    fn serves_row_ranges(&self) -> bool {
        true
    }
}

/// **`curl`-backed range fetching** (cargo feature `remote`, off by default): `curl -L -r a-b` for
/// ranges, HTTPS and redirects included (a Hugging Face `resolve` URL redirects to its CDN), and
/// `HF_TOKEN` as a bearer token when it is set. Nothing here runs unless a caller constructs it.
#[cfg(feature = "remote")]
pub struct CurlFetcher {
    token: Option<String>,
}

#[cfg(feature = "remote")]
impl CurlFetcher {
    pub fn new() -> Self {
        Self { token: std::env::var("HF_TOKEN").ok().filter(|t| !t.is_empty()) }
    }

    fn curl(&self, args: &[&str], url: &str) -> Result<Vec<u8>> {
        let mut c = std::process::Command::new("curl");
        c.args(["--silent", "--show-error", "--location", "--fail"]);
        if let Some(t) = &self.token {
            c.arg("--header").arg(format!("Authorization: Bearer {t}"));
        }
        c.args(args).arg("--").arg(url);
        let out = c.output().map_err(|e| LowerError::Io(format!("curl: {e}")))?;
        if !out.status.success() {
            return Err(LowerError::Io(format!("curl {url}: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(out.stdout)
    }
}

#[cfg(feature = "remote")]
impl Default for CurlFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "remote")]
impl RangeFetcher for CurlFetcher {
    fn len(&self, url: &str) -> Result<Option<u64>> {
        // A one-byte range answers `Content-Range: bytes 0-0/<len>` (the headers of the last hop).
        let mut c = std::process::Command::new("curl");
        c.args(["--silent", "--location", "--dump-header", "-", "--output", "/dev/null", "--range", "0-0"]);
        if let Some(t) = &self.token {
            c.arg("--header").arg(format!("Authorization: Bearer {t}"));
        }
        c.arg("--").arg(url);
        let out = c.output().map_err(|e| LowerError::Io(format!("curl: {e}")))?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        // The status of the last response in the header dump.
        let status = text.lines().filter(|l| l.starts_with("HTTP/")).next_back().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("");
        if status == "404" {
            return Ok(None);
        }
        let total = text
            .lines()
            .filter_map(|l| l.to_ascii_lowercase().strip_prefix("content-range:").map(str::to_string))
            .next_back()
            .and_then(|v| v.rsplit('/').next().and_then(|n| n.trim().parse::<u64>().ok()));
        match (status, total) {
            ("206", Some(n)) => Ok(Some(n)),
            // A server that ignores ranges answers 200 with the length.
            ("200", _) => text
                .lines()
                .filter_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(str::to_string))
                .next_back()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Some)
                .ok_or_else(|| LowerError::Io(format!("{url}: no length"))),
            _ => Err(LowerError::Io(format!("{url}: HTTP {status}"))),
        }
    }

    fn fetch(&self, url: &str, range: Range<u64>) -> Result<Vec<u8>> {
        if range.is_empty() {
            return Ok(Vec::new());
        }
        let r = format!("{}-{}", range.start, range.end - 1);
        let b = self.curl(&["--range", &r], url)?;
        if b.len() as u64 != range.end - range.start {
            return Err(LowerError::Io(format!("{url}: asked {range:?}, the server sent {} bytes (no range support?)", b.len())));
        }
        Ok(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weights::tests::safetensors_bytes;

    fn repo(shards: &[(&str, Vec<(&str, &str, Vec<usize>, Vec<u8>)>)]) -> (MemoryFetcher, Vec<(String, Vec<u8>)>) {
        let mut files = BTreeMap::new();
        let mut wm = serde_json::Map::new();
        for (file, tensors) in shards {
            files.insert(format!("https://example.invalid/m/{file}"), safetensors_bytes(tensors));
            for (n, ..) in tensors {
                wm.insert(n.to_string(), serde_json::json!(file));
            }
        }
        if shards.len() > 1 {
            files.insert(
                "https://example.invalid/m/model.safetensors.index.json".into(),
                serde_json::json!({"metadata": {}, "weight_map": wm}).to_string().into_bytes(),
            );
        }
        let list = files.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        (MemoryFetcher::new(files), list)
    }

    fn f32b(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn a_remote_repository_is_read_by_ranges_and_equals_the_local_one() {
        let big: Vec<f32> = (0..4096).map(|i| i as f32).collect();
        let (fetcher, _) = repo(&[
            ("model-00001-of-00002.safetensors", vec![("x.weight", "F32", vec![4, 1024], f32b(&big)), ("n.weight", "F32", vec![2], f32b(&[1.0, 2.0]))]),
            ("model-00002-of-00002.safetensors", vec![("y.weight", "BF16", vec![2, 3], vec![0u8; 12])]),
        ]);
        let ck = RemoteCheckpoint::open(Box::new(fetcher), "https://example.invalid/m").unwrap();
        // Opening fetched the index and two headers, not the 16 KiB tensor.
        assert!(ck.fetched_bytes() < 1024, "headers only: {}", ck.fetched_bytes());
        assert_eq!(ck.names(), vec!["n.weight", "x.weight", "y.weight"]);
        let m = ck.metadata("x.weight").unwrap();
        assert_eq!((m.dtype.as_str(), m.shape.clone(), m.bytes), ("F32", vec![4, 1024], 16384));
        assert_eq!(m.rows_cols(), (4, 1024));
        assert!(ck.fetched_bytes() < 1024);
        // Two rows of the big tensor: only their bytes are fetched.
        let before = ck.fetched_bytes();
        let r = ck.load_rows("x.weight", 1..3).unwrap();
        assert_eq!(r.shape, vec![2, 1024]);
        assert_eq!(&r.data[..3], &[1024.0, 1025.0, 1026.0]);
        assert_eq!(ck.fetched_bytes() - before, 2 * 1024 * 4);
        assert_eq!(ck.read_slice("n.weight", 4..8).unwrap(), 2.0f32.to_le_bytes());
        assert_eq!(ck.load("n.weight").unwrap().data, vec![1.0, 2.0]);
        assert!(ck.read_slice("n.weight", 4..12).is_err());
        assert!(ck.load_rows("x.weight", 3..5).is_err());
    }

    #[test]
    fn a_lone_model_safetensors_has_no_index() {
        let (fetcher, _) = repo(&[("model.safetensors", vec![("a", "F32", vec![2], f32b(&[3.0, 4.0]))])]);
        let ck = RemoteCheckpoint::open(Box::new(fetcher), "https://example.invalid/m/").unwrap();
        assert_eq!(ck.load("a").unwrap().data, vec![3.0, 4.0]);
    }

    #[test]
    fn a_hostile_header_is_refused_before_it_is_fetched() {
        let mut bad = (u64::MAX / 2).to_le_bytes().to_vec();
        bad.extend([0u8; 16]);
        let mut files = BTreeMap::new();
        files.insert("https://example.invalid/m/model.safetensors".to_string(), bad);
        assert!(RemoteCheckpoint::open(Box::new(MemoryFetcher::new(files)), "https://example.invalid/m").is_err());
        assert!(RemoteCheckpoint::open(Box::new(MemoryFetcher::default()), "https://example.invalid/m").is_err());
    }

    /// The `curl` fetcher against a loopback server that honours `Range` (no network beyond
    /// 127.0.0.1; compiled only with the `remote` feature, which is off by default).
    #[cfg(feature = "remote")]
    #[test]
    fn the_curl_fetcher_reads_a_repository_by_ranges_over_loopback() {
        use std::io::{BufRead, BufReader, Write};
        let f32b = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let big: Vec<f32> = (0..2048).map(|i| i as f32).collect();
        let shard = safetensors_bytes(&[("x.weight", "F32", vec![2, 1024], f32b(&big))]);
        let files: BTreeMap<String, Vec<u8>> = [("/m/model.safetensors".to_string(), shard)].into_iter().collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let served = std::sync::Arc::new(AtomicU64::new(0));
        let served2 = served.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (mut w, mut r) = (stream.try_clone().expect("clone"), BufReader::new(stream));
                let mut line = String::new();
                r.read_line(&mut line).expect("request line");
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let mut range: Option<(usize, usize)> = None;
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (a, b) = v.trim().split_once('-').expect("range");
                        range = Some((a.parse().expect("start"), b.parse().expect("end")));
                    }
                }
                match files.get(&path) {
                    None => {
                        let _ = w.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    Some(body) => {
                        let (a, b) = range.unwrap_or((0, body.len() - 1));
                        let b = b.min(body.len() - 1);
                        served2.fetch_add((b - a + 1) as u64, Ordering::Relaxed);
                        let head = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {a}-{b}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len(),
                            b - a + 1
                        );
                        let _ = w.write_all(head.as_bytes());
                        let _ = w.write_all(&body[a..=b]);
                    }
                }
            }
        });
        let ck = RemoteCheckpoint::open(Box::new(CurlFetcher::new()), &format!("http://127.0.0.1:{port}/m")).expect("opens over loopback");
        assert_eq!(ck.names(), vec!["x.weight"]);
        // The header cost kilobytes of a 8 KiB file; one row costs one row.
        let before = served.load(Ordering::Relaxed);
        let row = ck.load_rows("x.weight", 1..2).expect("a row");
        assert_eq!(&row.data[..3], &[1024.0, 1025.0, 1026.0]);
        assert_eq!(served.load(Ordering::Relaxed) - before, 1024 * 4);
    }
}
