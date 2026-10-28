//! **A GGUF read by HTTPS range, never stored, verified twice** (genesis #8, 2026-10-16): the bytes the converter uses are the
//! bytes the publisher's LFS id names.
//!
//! The file is read in fixed windows (64 MiB by default) as the converter asks for tensors, in whatever order it asks.
//! * **In-order whole-file SHA-256.** The hasher advances window by window from the start; a window fetched ahead of it is
//!   hashed when the hasher reaches it (from the cache, or fetched again). [`HashedRanges::finish`] fetches whatever was never
//!   read and compares the digest with the pinned LFS id — before any output is produced from the bytes (the two-pass
//!   streamed conversion runs its output pass only after the measuring pass has finished this check).
//! * **Per-window checks.** The SHA-256 of every window is recorded the first time the window is fetched; a window fetched
//!   again (evicted from the cache, or in a second process given the recorded digests, [`HashedRanges::with_window_digests`])
//!   must have the same digest, so the bytes a later read uses are the bytes the whole-file digest covered.
//!
//! The fetch itself is behind [`RangeFetch`] (`curl` by range in the converter, memory in the tests): an offline tool, one
//! ranged GET, no TLS stack in the workspace for it.

use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Fetch bytes `[start, start + len)` of the remote file.
pub trait RangeFetch {
    fn fetch(&self, start: u64, len: u64) -> Result<Vec<u8>, String>;
}

/// `curl -sS -L --fail -r a-b URL` with retries. Anonymous: `HF_TOKEN` is removed from the child's environment.
pub struct CurlFetch {
    pub url: String,
}

impl RangeFetch for CurlFetch {
    fn fetch(&self, start: u64, len: u64) -> Result<Vec<u8>, String> {
        let range = format!("{start}-{}", start + len - 1);
        let mut last = String::new();
        for attempt in 0..5 {
            let out = std::process::Command::new("curl")
                .args(["-sS", "-L", "--fail", "--max-time", "1800", "-r", &range, &self.url])
                .env_remove("HF_TOKEN")
                .env_remove("HUGGING_FACE_HUB_TOKEN")
                .output()
                .map_err(|e| format!("curl: {e}"))?;
            if out.status.success() && out.stdout.len() as u64 == len {
                return Ok(out.stdout);
            }
            last = format!("status {:?}, {} of {len} bytes", out.status.code(), out.stdout.len());
            eprintln!("  range {range}: retrying (attempt {}): {last}", attempt + 1);
            std::thread::sleep(std::time::Duration::from_secs(2 << attempt));
        }
        Err(format!("range {range} of {}: {last}", self.url))
    }
}

/// The remote file, read by windows, hashed in order, every window's digest kept.
pub struct HashedRanges<F: RangeFetch> {
    fetch: F,
    len: u64,
    window: u64,
    cache_windows: usize,
    /// Most recently used last.
    cache: Vec<(u64, Arc<Vec<u8>>)>,
    hasher: Option<Sha256>,
    hashed_upto: u64,
    digests: BTreeMap<u64, [u8; 32]>,
    expect_sha256: Option<[u8; 32]>,
    fetched: u64,
    refetched: u64,
}

fn sha256(b: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(b));
    out
}

pub fn hex32(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn parse_hex32(s: &str) -> Result<[u8; 32], String> {
    let s = s.trim();
    if s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("`{s}` is not 64 hex digits"));
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

impl<F: RangeFetch> HashedRanges<F> {
    /// `len` bytes, windows of `window` bytes, `cache_windows` kept; `expect_sha256` is the pinned LFS id ([`finish`] compares).
    pub fn new(fetch: F, len: u64, window: u64, cache_windows: usize, expect_sha256: Option<[u8; 32]>) -> Self {
        HashedRanges {
            fetch,
            len,
            window: window.max(1),
            cache_windows: cache_windows.max(1),
            cache: Vec::new(),
            hasher: Some(Sha256::new()),
            hashed_upto: 0,
            digests: BTreeMap::new(),
            expect_sha256,
            fetched: 0,
            refetched: 0,
        }
    }

    /// Windows whose digests another process recorded (the measuring pass): every window this one fetches must match. The
    /// whole-file digest is then not recomputed here (that process compared it with the pin).
    pub fn with_window_digests(mut self, digests: BTreeMap<u64, [u8; 32]>) -> Self {
        self.digests = digests;
        self.hasher = None;
        self
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn window_bytes(&self) -> u64 {
        self.window
    }

    /// `(bytes fetched, windows fetched again)`.
    pub fn fetched(&self) -> (u64, u64) {
        (self.fetched, self.refetched)
    }

    /// The digest of every window seen so far, by window index.
    pub fn window_digests(&self) -> &BTreeMap<u64, [u8; 32]> {
        &self.digests
    }

    fn windows(&self) -> u64 {
        self.len.div_ceil(self.window)
    }

    fn get_window(&mut self, k: u64) -> Result<Arc<Vec<u8>>, String> {
        if let Some(i) = self.cache.iter().position(|(w, _)| *w == k) {
            let hit = self.cache.remove(i);
            let data = hit.1.clone();
            self.cache.push(hit);
            return Ok(data);
        }
        let start = k * self.window;
        let n = self.window.min(self.len - start);
        let bytes = self.fetch.fetch(start, n)?;
        if bytes.len() as u64 != n {
            return Err(format!("window {k}: {} of {n} bytes", bytes.len()));
        }
        self.fetched += n;
        let d = sha256(&bytes);
        match self.digests.get(&k) {
            Some(want) if *want != d => {
                return Err(format!(
                    "window {k} (bytes {start}..{}) changed between two reads: SHA-256 {} then {}",
                    start + n,
                    hex32(want),
                    hex32(&d)
                ));
            }
            Some(_) => self.refetched += 1,
            None => {
                self.digests.insert(k, d);
            }
        }
        let data = Arc::new(bytes);
        self.cache.push((k, data.clone()));
        if self.cache.len() > self.cache_windows {
            // Never evict a window the in-order hasher has not reached and that is the next it needs.
            let next = self.hashed_upto;
            if let Some(i) = self.cache.iter().position(|(w, _)| *w != next) {
                self.cache.remove(i);
            }
        }
        Ok(data)
    }

    /// Advance the in-order hasher over every window it can reach from the cache.
    fn advance(&mut self) {
        while self.hasher.is_some() && self.hashed_upto < self.windows() {
            let k = self.hashed_upto;
            let Some(data) = self.cache.iter().find(|(w, _)| *w == k).map(|(_, d)| d.clone()) else { break };
            if let Some(h) = self.hasher.as_mut() {
                h.update(&data[..]);
            }
            self.hashed_upto += 1;
        }
    }

    /// Bytes `[offset, offset + len)`.
    pub fn read(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, String> {
        let end = offset
            .checked_add(len as u64)
            .filter(|e| *e <= self.len)
            .ok_or_else(|| format!("bytes {offset}..+{len} past the end of a {}-byte file", self.len))?;
        // **Catch up.** A read a little ahead of the hasher (a tensor later in the same layer, the embedding after a head stored
        // first) would leave the windows in between unhashable until they are read, and they may be evicted by then: fetch and
        // hash them in order now. A far jump is left to `finish` (the windows are fetched again there and checked).
        let first = offset / self.window;
        if self.hasher.is_some() && first > self.hashed_upto && first - self.hashed_upto <= 4 * self.cache_windows as u64 {
            while self.hashed_upto < first {
                let k = self.hashed_upto;
                self.get_window(k)?;
                self.advance();
            }
        }
        let mut out = Vec::with_capacity(len);
        let mut at = offset;
        while at < end {
            let k = at / self.window;
            let w = self.get_window(k)?;
            let from = (at - k * self.window) as usize;
            let to = ((end - k * self.window) as usize).min(w.len());
            out.extend_from_slice(&w[from..to]);
            at = k * self.window + to as u64;
            self.advance();
        }
        Ok(out)
    }

    /// Hash whatever the reads never reached, in order, and compare the whole-file SHA-256 with the pin. Returns the digest
    /// (`None` when this reader was given another process's window digests and so verified windows only).
    pub fn finish(&mut self) -> Result<Option<[u8; 32]>, String> {
        if self.hasher.is_none() {
            return Ok(None);
        }
        while self.hashed_upto < self.windows() {
            let k = self.hashed_upto;
            self.get_window(k)?;
            self.advance();
        }
        let digest = {
            let h = self.hasher.take().expect("checked above");
            let mut out = [0u8; 32];
            out.copy_from_slice(&h.finalize());
            out
        };
        if let Some(want) = self.expect_sha256
            && want != digest
        {
            return Err(format!("whole-file SHA-256 {} is not the pinned {}", hex32(&digest), hex32(&want)));
        }
        Ok(Some(digest))
    }
}

/// Window digests as JSON (`{"window": W, "len": N, "digests": {"k": "hex"}}`), to hand the measuring pass's checks to the output
/// pass.
pub fn window_digests_json(window: u64, len: u64, digests: &BTreeMap<u64, [u8; 32]>) -> String {
    let d: serde_json::Map<String, serde_json::Value> =
        digests.iter().map(|(k, v)| (k.to_string(), serde_json::json!(hex32(v)))).collect();
    serde_json::json!({ "window": window, "len": len, "digests": d }).to_string()
}

/// [`window_digests_json`] read back; refuses a different window size or length.
pub fn window_digests_from_json(text: &str, window: u64, len: u64) -> Result<BTreeMap<u64, [u8; 32]>, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if v["window"].as_u64() != Some(window) || v["len"].as_u64() != Some(len) {
        return Err(format!("window digests for window {:?} / len {:?}, not {window} / {len}", v["window"], v["len"]));
    }
    let mut out = BTreeMap::new();
    for (k, h) in v["digests"].as_object().ok_or("no digests")? {
        out.insert(k.parse::<u64>().map_err(|e| e.to_string())?, parse_hex32(h.as_str().ok_or("a digest is not a string")?)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The file in memory; `flip` changes one byte from the given fetch count on (a server whose bytes change).
    struct Mem {
        data: Vec<u8>,
        calls: RefCell<u64>,
        flip_from: Option<u64>,
    }

    impl RangeFetch for Mem {
        fn fetch(&self, start: u64, len: u64) -> Result<Vec<u8>, String> {
            *self.calls.borrow_mut() += 1;
            let mut v = self.data[start as usize..(start + len) as usize].to_vec();
            if self.flip_from.is_some_and(|f| *self.calls.borrow() > f) {
                v[0] ^= 1;
            }
            Ok(v)
        }
    }

    fn file(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect()
    }

    #[test]
    fn out_of_order_reads_hash_the_whole_file_in_order_and_match_the_pin() {
        let data = file(10_000);
        let pin = sha256(&data);
        let mut r =
            HashedRanges::new(Mem { data: data.clone(), calls: RefCell::new(0), flip_from: None }, 10_000, 1_000, 2, Some(pin));
        // Reads out of order, across windows, with evictions (a cache of two windows).
        for (off, len) in [(8_500u64, 1_200usize), (0, 10), (4_990, 30), (2_000, 3_000), (9_999, 1)] {
            assert_eq!(r.read(off, len).unwrap(), data[off as usize..off as usize + len]);
        }
        assert_eq!(r.finish().unwrap(), Some(pin));
        assert!(r.fetched().1 > 0, "evicted windows were fetched again and checked");
        // A wrong pin is refused.
        let mut bad =
            HashedRanges::new(Mem { data: data.clone(), calls: RefCell::new(0), flip_from: None }, 10_000, 1_000, 2, Some([7; 32]));
        bad.read(0, 10).unwrap();
        assert!(bad.finish().unwrap_err().contains("not the pinned"));
    }

    #[test]
    fn a_window_that_changes_between_reads_is_refused_in_this_process_and_in_the_next() {
        let data = file(4_000);
        // Changes from the third fetch on: window 0 fetched, evicted, fetched again → refused.
        let mut r = HashedRanges::new(Mem { data: data.clone(), calls: RefCell::new(0), flip_from: Some(2) }, 4_000, 1_000, 1, None);
        r.read(0, 10).unwrap();
        r.read(1_500, 10).unwrap();
        assert!(r.read(5, 10).unwrap_err().contains("changed between two reads"));
        // The output pass gets the measuring pass's digests: a changed window is refused there too.
        let mut first = HashedRanges::new(Mem { data: data.clone(), calls: RefCell::new(0), flip_from: None }, 4_000, 1_000, 4, None);
        first.read(0, 4_000).unwrap();
        first.finish().unwrap();
        let json = window_digests_json(1_000, 4_000, first.window_digests());
        let digests = window_digests_from_json(&json, 1_000, 4_000).unwrap();
        let mut ok = HashedRanges::new(Mem { data: data.clone(), calls: RefCell::new(0), flip_from: None }, 4_000, 1_000, 4, None)
            .with_window_digests(digests.clone());
        assert_eq!(ok.read(2_500, 100).unwrap(), data[2_500..2_600]);
        assert_eq!(ok.finish().unwrap(), None);
        let mut changed = HashedRanges::new(Mem { data, calls: RefCell::new(0), flip_from: Some(0) }, 4_000, 1_000, 4, None)
            .with_window_digests(digests);
        assert!(changed.read(2_500, 100).unwrap_err().contains("changed between two reads"));
        assert!(window_digests_from_json(&json, 512, 4_000).is_err(), "another window size is refused");
    }
}
