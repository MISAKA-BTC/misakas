//! **Agent C4 — hostile evidence providers and a hostile HTTP peer** (RFC-0009 stage B/C: DA transport, redemption board).
//!
//! A DA provider is never a computation authority and never a truth authority: every byte it returns is judged against the claim's
//! own roots. A peer on the wire can send anything. Expected results are stated before each run; verdicts in the record.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_remote::evidence::{ChunkProvider, ClaimRoots, EvidenceManifestV1, ManifestLimits, ProviderError};
use misaka_palw_remote::transport::{
    EvidenceProvider, HttpProvider, HttpResponse, HttpTransport, StdHttp, fetch_claim_material_any, sync_redemptions_v1,
};

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adv-c4-transport-{name}-{}", std::process::id()));
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

fn claim_hex() -> String {
    "ab".repeat(64)
}

/// A provider with a script: it serves `manifest` and a function of (index) for chunks.
struct Scripted {
    name: &'static str,
    manifest: Option<EvidenceManifestV1>,
    chunk: Box<dyn Fn(u32) -> Result<Vec<u8>, ProviderError> + Send + Sync>,
}

impl ChunkProvider for Scripted {
    fn provider_id(&self) -> &str {
        self.name
    }
    fn fetch_chunk(&self, _: Hash64, index: u32) -> Result<Vec<u8>, ProviderError> {
        (self.chunk)(index)
    }
}

impl EvidenceProvider for Scripted {
    fn manifest_for(&self, _: &str) -> Result<Option<EvidenceManifestV1>, ProviderError> {
        Ok(self.manifest.clone())
    }
    fn put_chunk(&self, _: Hash64, _: u32, _: &[u8]) -> Result<(), ProviderError> {
        Ok(())
    }
    fn put_manifest(&self, _: &str, _: &EvidenceManifestV1) -> Result<(), ProviderError> {
        Ok(())
    }
    fn list_redemptions(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["../../escape".into(), "a".repeat(127), "z".repeat(128), "ab".repeat(64), "/etc/passwd".into(), String::new()])
    }
    fn get_redemption(&self, _: &str) -> Result<Option<Vec<u8>>, ProviderError> {
        Ok(Some(vec![0xFF; 64]))
    }
    fn put_redemption(&self, _: &str, _: &[u8]) -> Result<(), ProviderError> {
        Ok(())
    }
}

fn good(name: &'static str) -> Scripted {
    let c = chunks();
    Scripted {
        name,
        manifest: Some(manifest()),
        chunk: Box::new(move |i| c.get(i as usize).cloned().ok_or(ProviderError("no such chunk".into()))),
    }
}

/// EXPECTED (before running): material is whatever hashes to the claim's roots and nothing else. A provider that flips a byte of a
/// chunk, serves one chunk's bytes under another index, truncates, pads, serves a manifest of another claim (other roots, other
/// network, other chunk count) or is silent is recorded and skipped; a good copy elsewhere is used; and when ONLY hostile providers
/// exist the fetch is an error and no byte is returned. VERDICT: PASS.
#[test]
fn hostile_providers_cannot_make_a_client_accept_material_the_claim_does_not_commit() {
    let c = chunks();
    let flipped: Vec<Vec<u8>> = c
        .iter()
        .map(|x| {
            let mut y = x.clone();
            y[0] ^= 1;
            y
        })
        .collect();
    let (c1, c2) = (c.clone(), c.clone());
    let bad_flip = Scripted { name: "flip", manifest: Some(manifest()), chunk: Box::new(move |i| Ok(flipped[i as usize].clone())) };
    let bad_swap =
        Scripted { name: "swap", manifest: Some(manifest()), chunk: Box::new(move |i| Ok(c1[(i as usize + 1) % 3].clone())) };
    let bad_trunc = Scripted {
        name: "trunc",
        manifest: Some(manifest()),
        chunk: Box::new(move |i| Ok(c2[i as usize][..c2[i as usize].len() - 1].to_vec())),
    };
    let c3 = c.clone();
    let bad_pad = Scripted {
        name: "pad",
        manifest: Some(manifest()),
        chunk: Box::new(move |i| {
            let mut y = c3[i as usize].clone();
            y.push(0);
            Ok(y)
        }),
    };
    let silent = Scripted { name: "silent", manifest: Some(manifest()), chunk: Box::new(|_| Err(ProviderError("down".into()))) };
    // Manifests of OTHER claims: other roots, other network, other chunk count.
    let other_roots =
        EvidenceManifestV1::build(h(9), &TransactionOutpoint::new(h(0xB0), 1), &[5u8; 32], h(0x11), h(2), h(3), 3, 10_000, &chunks());
    let other_net =
        EvidenceManifestV1::build(h(0x99), &TransactionOutpoint::new(h(0xB0), 1), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &chunks());
    let two = vec![vec![1u8; 10], vec![2u8; 10]];
    let other_count =
        EvidenceManifestV1::build(h(9), &TransactionOutpoint::new(h(0xB0), 1), &[5u8; 32], h(1), h(2), h(3), 2, 10_000, &two);
    let c4 = c.clone();
    let wrong_manifests: Vec<Scripted> = [other_roots, other_net, other_count]
        .into_iter()
        .enumerate()
        .map(|(k, m)| {
            let c = c4.clone();
            // These providers hold a manifest of ANOTHER claim and serve garbage for every chunk.
            let _ = &c;
            Scripted {
                name: ["roots", "net", "count"][k],
                manifest: Some(m),
                chunk: Box::new(move |i| Ok(vec![0x66; 100 + i as usize])),
            }
        })
        .collect();
    let limits = ManifestLimits::default();

    let mut hostile: Vec<&dyn EvidenceProvider> = vec![&bad_flip, &bad_swap, &bad_trunc, &bad_pad, &silent];
    hostile.extend(wrong_manifests.iter().map(|p| p as &dyn EvidenceProvider));
    let r = fetch_claim_material_any(&hostile, &claim_hex(), &roots(), &limits, h(7));
    assert!(r.is_err(), "only hostile providers: no byte may be returned, got {:?}", r.as_ref().map(|(b, _)| b.len()));
    // Whatever succeeds is exactly the committed material: a provider that serves the RIGHT bytes under any manifest is no attack.
    if let Ok((b, _)) = &r {
        assert_eq!(b, &chunks().concat());
    }

    let g = good("good");
    let mut mixed = hostile.clone();
    mixed.push(&g);
    let (bytes, report) = fetch_claim_material_any(&mixed, &claim_hex(), &roots(), &limits, h(7)).expect("a good copy exists");
    assert_eq!(bytes, chunks().concat());
    eprintln!("served by {:?}", report.served_by.len());
    // The claim's roots decide, not the manifest's own: a manifest consistent with itself but for other roots is refused above; the
    // same good bytes under a claim with other roots are refused too.
    let mut other_claim = roots();
    other_claim.trace_root = h(0x42);
    assert!(fetch_claim_material_any(&[&g as &dyn EvidenceProvider], &claim_hex(), &other_claim, &limits, h(7)).is_err());
}

/// EXPECTED (before running): a provider's redemption listing and bundles are hints: names that are not exactly 128 hex characters
/// (path separators, `..`, absolute paths, wrong length, empty) and bytes that are not a bundle of that claim write NOTHING, inside
/// or outside the target directory. VERDICT: PASS.
#[test]
fn a_hostile_redemption_listing_writes_nothing_anywhere() {
    let parent = scratch("redeem");
    let into = parent.join("board");
    let evil = Scripted { name: "evil", manifest: None, chunk: Box::new(|_| Err(ProviderError("x".into()))) };
    let before: Vec<_> = std::fs::read_dir(&parent).unwrap().flatten().map(|e| e.file_name()).collect();
    let report = sync_redemptions_v1(&[&evil as &dyn EvidenceProvider], &into, None, Some(1)).expect("io ok");
    assert!(report.written.is_empty(), "{report:?}");
    let after: Vec<_> = std::fs::read_dir(&parent).unwrap().flatten().map(|e| e.file_name()).collect();
    assert_eq!(before.len() + 1, after.len(), "only the (empty) board directory was created in the parent");
    assert!(std::fs::read_dir(&into).unwrap().next().is_none(), "nothing was mirrored");
    assert!(!parent.join("escape").exists() && !parent.parent().unwrap().join("escape").exists());
}

// ── a hostile peer on the wire ───────────────────────────────────────────────────────────────────────────────────────────

/// A one-shot TCP server that, once a request has been read, plays `script` and closes.
fn hostile_server(script: impl Fn(&mut std::net::TcpStream) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            script(&mut s);
        }
    });
    format!("http://{addr}")
}

/// EXPECTED (before running): the client's `max_response` bound is a promise: a peer that announces a gigantic Content-Length,
/// streams endless data without one, sends an unterminated header block, sends chunked encoding with absurd sizes, or answers
/// garbage never makes `StdHttp::request` return more than `max_response` bytes, hang past its timeout, or panic.
#[test]
fn a_hostile_http_peer_cannot_make_the_client_buffer_more_than_it_asked_for() {
    let max = 64 * 1024;
    let cases: Vec<(&str, Box<dyn Fn(&mut std::net::TcpStream) + Send>)> = vec![
        (
            "huge content-length",
            Box::new(|s| {
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 99999999999\r\n\r\n");
                let junk = vec![0x41u8; 1 << 20];
                for _ in 0..16 {
                    if s.write_all(&junk).is_err() {
                        break;
                    }
                }
            }),
        ),
        (
            "endless body",
            Box::new(|s| {
                let _ = write!(s, "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                let junk = vec![0x42u8; 1 << 20];
                for _ in 0..16 {
                    if s.write_all(&junk).is_err() {
                        break;
                    }
                }
            }),
        ),
        (
            "unterminated header",
            Box::new(|s| {
                let _ = write!(s, "HTTP/1.1 200 OK\r\n");
                let junk = vec![b'X'; 1 << 20];
                for _ in 0..8 {
                    if s.write_all(&junk).is_err() {
                        break;
                    }
                }
            }),
        ),
        (
            "chunked, absurd size",
            Box::new(|s| {
                let _ = write!(s, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFF\r\n");
                let junk = vec![0x43u8; 1 << 20];
                for _ in 0..16 {
                    if s.write_all(&junk).is_err() {
                        break;
                    }
                }
            }),
        ),
        (
            "garbage",
            Box::new(|s| {
                let _ = s.write_all(&[0xFF, 0xFE, 0x00, 0x01, 0x02]);
            }),
        ),
        (
            "redirect",
            Box::new(|s| {
                let _ = write!(s, "HTTP/1.1 302 Found\r\nLocation: file:///etc/passwd\r\nContent-Length: 0\r\n\r\n");
            }),
        ),
    ];
    let http = StdHttp { timeout: Duration::from_secs(4) };
    for (what, script) in cases {
        let base = hostile_server(script);
        let started = Instant::now();
        let r = std::panic::catch_unwind(|| http.request("GET", &format!("{base}/claims/x.manifest"), None, max));
        let r = r.unwrap_or_else(|_| panic!("{what}: the client panicked"));
        assert!(started.elapsed() < Duration::from_secs(12), "{what}: took {:?}", started.elapsed());
        match r {
            Ok(HttpResponse { status, body }) => {
                assert!(body.len() <= max, "{what}: returned {} bytes for a {max}-byte bound", body.len());
                assert!(status != 200 || what == "redirect" || what == "garbage" || body.len() <= max, "{what}");
            }
            Err(_) => {}
        }
    }
    let _ = Arc::new(HttpProvider::new("http://127.0.0.1:1"));
}
