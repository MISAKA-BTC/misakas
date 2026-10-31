//! **One remote file read by windows and hashed in order** (genesis 8, #6: the 54.66 GB bf16 GGUF streamed end to end).
//!
//! A conversion reads a file's tensors in the order the lowering reaches them, twice (calibration, then materialisation),
//! and the file is too large to keep. This source reads it from any [`RangeFetcher`] (an HTTPS `Range` against the Hugging
//! Face `resolve` URL at the pinned revision) in fixed windows, keeps the most recent few in memory, and proves at the end that
//! every byte it served is the publisher's file:
//!
//! * **Hashed in order.** The whole-file SHA-256 is computed over the windows strictly in file order. A read that skips ahead
//!   first fetches (and caches) the windows in between, so every window is hashed at its first fetch, and its own SHA-256 is
//!   recorded.
//! * **Every refetch re-checked.** A window evicted and fetched again (the second pass, a backward read) must have the digest
//!   recorded when it was hashed: a server that serves other bytes the second time is refused, not believed.
//! * **Compared at the end.** [`ByteSourceV1::finish`] hashes whatever the conversion never read (the tail, dropped layers) and
//!   compares the whole-file SHA-256 with the LFS id the registry pins. The converter calls it before it writes the artifact's
//!   index: a mismatch publishes no identity.
//! * **Bounded.** Memory is `keep` windows; no byte touches the disk. A fetch that returns another length is refused; a failed
//!   fetch is retried a bounded number of times.

use super::RangeFetcher;
use crate::error::{LowerError, Result};
use sha2::{Digest as _, Sha256};
use std::sync::{Arc, Mutex};

/// Random access to a file's bytes: a local file, or a verified remote one.
pub trait ByteSourceV1: Send + Sync + std::fmt::Debug {
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()>;
    /// The conversion has read everything it will: a verifying source completes its check here.
    fn finish(&self) -> Result<()> {
        Ok(())
    }
    /// What was fetched (instrumentation): `(bytes fetched, windows refetched)`.
    fn fetched(&self) -> (u64, u64) {
        (0, 0)
    }
}

impl ByteSourceV1 for std::fs::File {
    fn len(&self) -> u64 {
        self.metadata().map(|m| m.len()).unwrap_or(0)
    }
    fn read_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
        super::read_exact_at(self, buf, offset)
    }
}

struct Win {
    k: u64,
    bytes: Arc<Vec<u8>>,
    last_use: u64,
}

struct St {
    cache: Vec<Win>,
    tick: u64,
    /// The next window the whole-file hash consumes.
    next: u64,
    sha: Sha256,
    /// Each hashed window's own SHA-256 (index = window).
    digests: Vec<[u8; 32]>,
    fetched: u64,
    refetched: u64,
    verdict: Option<std::result::Result<(), String>>,
}

/// The remote file (see the module docs).
pub struct RemoteHashedFileV1 {
    fetcher: Box<dyn RangeFetcher>,
    url: String,
    len: u64,
    expect_sha256: [u8; 32],
    window: u64,
    keep: usize,
    retries: u32,
    st: Mutex<St>,
}

impl std::fmt::Debug for RemoteHashedFileV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteHashedFileV1").field("url", &self.url).field("len", &self.len).field("window", &self.window).finish()
    }
}

fn io_err(m: impl Into<String>) -> std::io::Error {
    std::io::Error::other(m.into())
}

impl RemoteHashedFileV1 {
    /// `url` of `len` bytes whose SHA-256 must be `expect_sha256`, read in windows of `window` bytes, `keep` of them cached.
    pub fn new(
        fetcher: Box<dyn RangeFetcher>,
        url: &str,
        len: u64,
        expect_sha256: [u8; 32],
        window: u64,
        keep: usize,
    ) -> Result<Self> {
        if !(1 << 20..=1 << 30).contains(&window) {
            return Err(LowerError::bad(format!("a window of {window} bytes (1 MiB .. 1 GiB)")));
        }
        if len == 0 {
            return Err(LowerError::bad("an empty remote file"));
        }
        match fetcher.len(url)? {
            Some(n) if n == len => {}
            Some(n) => return Err(LowerError::Io(format!("{url}: the server says {n} bytes, the pin says {len}"))),
            None => return Err(LowerError::Io(format!("{url}: not found"))),
        }
        let n_windows = len.div_ceil(window) as usize;
        Ok(Self {
            fetcher,
            url: url.to_string(),
            len,
            expect_sha256,
            window,
            keep: keep.max(2),
            retries: 4,
            st: Mutex::new(St {
                cache: Vec::new(),
                tick: 0,
                next: 0,
                sha: Sha256::new(),
                digests: Vec::with_capacity(n_windows),
                fetched: 0,
                refetched: 0,
                verdict: None,
            }),
        })
    }

    fn n_windows(&self) -> u64 {
        self.len.div_ceil(self.window)
    }

    fn window_range(&self, k: u64) -> std::ops::Range<u64> {
        let a = k * self.window;
        a..(a + self.window).min(self.len)
    }

    fn fetch(&self, k: u64) -> std::io::Result<Vec<u8>> {
        let r = self.window_range(k);
        let want = (r.end - r.start) as usize;
        let mut last = String::new();
        for attempt in 0..=self.retries {
            match self.fetcher.fetch(&self.url, r.clone()) {
                Ok(b) if b.len() == want => return Ok(b),
                Ok(b) => last = format!("window {k}: {} bytes, asked {want}", b.len()),
                Err(e) => last = format!("window {k}: {e}"),
            }
            if attempt < self.retries {
                std::thread::sleep(std::time::Duration::from_secs(2u64 << attempt));
            }
        }
        Err(io_err(format!("{}: {last}", self.url)))
    }

    fn remember(&self, st: &mut St, k: u64, bytes: Arc<Vec<u8>>) {
        st.tick += 1;
        let tick = st.tick;
        if st.cache.len() >= self.keep
            && let Some((i, _)) = st.cache.iter().enumerate().min_by_key(|(_, w)| w.last_use)
        {
            st.cache.swap_remove(i);
        }
        st.cache.push(Win { k, bytes, last_use: tick });
    }

    /// Hash window `st.next` (fetching it) and advance.
    fn hash_next(&self, st: &mut St) -> std::io::Result<Arc<Vec<u8>>> {
        let k = st.next;
        let b = self.fetch(k)?;
        st.fetched += b.len() as u64;
        st.sha.update(&b);
        let mut d = [0u8; 32];
        d.copy_from_slice(&Sha256::digest(&b));
        st.digests.push(d);
        st.next += 1;
        let b = Arc::new(b);
        self.remember(st, k, b.clone());
        Ok(b)
    }

    fn window(&self, st: &mut St, k: u64) -> std::io::Result<Arc<Vec<u8>>> {
        st.tick += 1;
        let tick = st.tick;
        if let Some(w) = st.cache.iter_mut().find(|w| w.k == k) {
            w.last_use = tick;
            return Ok(w.bytes.clone());
        }
        if k >= st.next {
            // Every window before k is hashed first, in order.
            while st.next < k {
                self.hash_next(st)?;
            }
            return self.hash_next(st);
        }
        // Hashed before and evicted: the bytes must be the ones hashed.
        let b = self.fetch(k)?;
        st.fetched += b.len() as u64;
        st.refetched += 1;
        if Sha256::digest(&b).as_slice() != st.digests[k as usize] {
            return Err(io_err(format!("{}: window {k} changed between two reads", self.url)));
        }
        let b = Arc::new(b);
        self.remember(st, k, b.clone());
        Ok(b)
    }
}

impl ByteSourceV1 for RemoteHashedFileV1 {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
        let end = offset
            .checked_add(buf.len() as u64)
            .filter(|e| *e <= self.len)
            .ok_or_else(|| io_err("a read past the end of the file"))?;
        let mut st = self.st.lock().map_err(|_| io_err("a poisoned lock"))?;
        let mut at = offset;
        let mut done = 0usize;
        while at < end {
            let k = at / self.window;
            let w = self.window(&mut st, k)?;
            let within = (at - k * self.window) as usize;
            let n = (w.len() - within).min(buf.len() - done);
            buf[done..done + n].copy_from_slice(&w[within..within + n]);
            done += n;
            at += n as u64;
        }
        Ok(())
    }

    fn finish(&self) -> Result<()> {
        let mut st = self.st.lock().map_err(|_| LowerError::eval("a poisoned lock"))?;
        if let Some(v) = &st.verdict {
            return v.clone().map_err(LowerError::Io);
        }
        while st.next < self.n_windows() {
            self.hash_next(&mut st).map_err(|e| LowerError::Io(e.to_string()))?;
        }
        let got = st.sha.clone().finalize();
        let verdict = if got.as_slice() == self.expect_sha256 {
            Ok(())
        } else {
            let hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
            Err(format!("{}: the whole-file SHA-256 is {hex}, not the pinned LFS id", self.url))
        };
        st.verdict = Some(verdict.clone());
        verdict.map_err(LowerError::Io)
    }

    fn fetched(&self) -> (u64, u64) {
        self.st.lock().map(|s| (s.fetched, s.refetched)).unwrap_or((0, 0))
    }
}

/// A sequential reader over a byte source (the GGUF header parser reads through it; its bytes are hashed like any other).
pub struct ByteSourceReaderV1<'a> {
    pub src: &'a dyn ByteSourceV1,
    pub pos: u64,
    /// The most bytes this reader hands out (a header cap).
    pub limit: u64,
}

impl std::io::Read for ByteSourceReaderV1<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let end = self.src.len().min(self.limit);
        if self.pos >= end {
            return Ok(0);
        }
        let n = (out.len() as u64).min(end - self.pos).min(8 << 20) as usize;
        self.src.read_at(&mut out[..n], self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weights::MemoryFetcher;
    use std::collections::BTreeMap;

    fn file(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 131 % 251) as u8).collect()
    }

    fn src(bytes: &[u8], sha: [u8; 32], keep: usize) -> RemoteHashedFileV1 {
        let f = MemoryFetcher::new(BTreeMap::from([("u".to_string(), bytes.to_vec())]));
        RemoteHashedFileV1::new(Box::new(f), "u", bytes.len() as u64, sha, 1 << 20, keep).unwrap()
    }

    #[test]
    fn reads_in_any_order_are_served_and_the_file_hash_is_checked_at_the_end() {
        let bytes = file((5 << 20) + 12345);
        let mut sha = [0u8; 32];
        sha.copy_from_slice(&Sha256::digest(&bytes));
        let s = src(&bytes, sha, 2);
        // Ahead, back, across a cut, and a second pass after eviction.
        for (off, len) in [(3 << 20, 100usize), (10, 50), ((1 << 20) - 7, 20), ((4 << 20) + 1, 70_000), (0, 3 << 20)] {
            let mut b = vec![0u8; len];
            s.read_at(&mut b, off as u64).unwrap();
            assert_eq!(b, bytes[off..off + len]);
        }
        s.finish().unwrap();
        assert!(s.fetched().1 > 0, "evicted windows were refetched and re-checked");
        // A wrong pin fails at the end, and only there.
        let bad = src(&bytes, [0u8; 32], 2);
        let mut b = vec![0u8; 10];
        bad.read_at(&mut b, 0).unwrap();
        assert!(bad.finish().is_err());
    }
}
