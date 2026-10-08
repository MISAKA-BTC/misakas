//! **Request idempotency for the one route that costs an inference and an exposure** (RFC-0001 §2.7; the matrix row
//! "idempotency key / dedup" was a DESIGN_GAP).
//!
//! # Why a retry is not harmless on this lane
//!
//! A client whose connection drops after it sent `POST /v1/chat/completions` cannot tell "the gateway never ran it" from
//! "the gateway ran it and the answer was lost", and a stock SDK retries. Without a key the retry is a SECOND job:
//! a second inference, a second random `job_nonce` — so a second, different claim id for the same work — and a second
//! charge against the public-job budget, for a commitment the chain then refuses as `DuplicateWork` (the work identity
//! is `(class, prompt, bond)`, not the nonce: `fp_work_id_v1`). The retry loses the user their answer's claim and the
//! operator an inference.
//!
//! # The rule
//!
//! `Idempotency-Key: <key>` (Stripe's header; the OpenAI SDKs send it on retry) plus the request's canonical digest:
//!
//! | key seen before | same canonical request | outcome |
//! |---|---|---|
//! | no | — | runs once; the finished response is stored under the key |
//! | yes, finished | yes | **the stored response, byte for byte** — the same `fp_claim_id`, no inference, no budget charge, no second commitment |
//! | yes, finished | no | refused (409): the key names a different request |
//! | yes, running | yes | refused (409): the first attempt is still in flight; retry shortly |
//! | yes, running | no | refused (409) |
//!
//! A request that FAILED or was CANCELLED leaves nothing behind (the reservation is released), so its retry runs.
//!
//! * **The key is never written down.** The record is filed under a keyed hash of it; a key is a client's secret-ish
//!   token and the outbox is read by operators and the rail.
//! * **The store is a cache over the outbox directory, not the other way round**: a finished record survives a restart
//!   (the retry after a crash still finds its answer) and is dropped after [`TTL_SECS`] or when the table is full.
//! * **Scope**: a key is global to the gateway (there are no accounts). A stranger who guesses a key learns only that
//!   it exists: a response is returned only to a request whose canonical digest matches the stored one.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use kaspa_hashes::Hash64;

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";
/// Longest accepted key, in bytes. A UUID is 36.
pub const MAX_KEY_BYTES: usize = 128;
/// How long a finished response is kept for replay.
pub const TTL_SECS: u64 = 24 * 60 * 60;
/// Most records held in memory (finished ones are evicted oldest first; a running one is never evicted).
pub const MAX_ENTRIES: usize = 4_096;
pub const SCHEMA: &str = "misaka.palw.gateway-idempotency.v1";

const DOMAIN_KEY: &[u8] = b"misaka-palw/gateway/idempotency-key/v1";
const DOMAIN_REQUEST: &[u8] = b"misaka-palw/gateway/request-digest/v1";

/// A validated key: 1..=[`MAX_KEY_BYTES`] bytes of printable ASCII with no space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.is_empty() || raw.len() > MAX_KEY_BYTES {
            return Err(format!("the Idempotency-Key must be 1..={MAX_KEY_BYTES} bytes"));
        }
        if !raw.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
            return Err("the Idempotency-Key must be printable ASCII without spaces".into());
        }
        Ok(Self(raw.to_string()))
    }

    /// The hash the record is filed under; the key itself is never stored.
    pub fn id(&self) -> Hash64 {
        kaspa_hashes::blake2b_512_keyed(DOMAIN_KEY, self.0.as_bytes())
    }
}

/// **The canonical digest of a chat request**: its JSON in RFC 8785 form with the transport-only members removed.
///
/// `stream` and `stream_options` are removed — they choose how the SAME answer is delivered, so a client that retries a
/// timed-out non-streaming call as a streaming one is asking for the same inference. Everything else is in: the messages
/// (a decoded image's pixels included), the model, `max_tokens`, the sampling members, `seed`, `stop`, `tools`,
/// `response_format`, `derive` — a request that differs in any of them is a different request, and the digest changes.
/// Key order and insignificant whitespace do not matter.
pub fn request_digest(body: &[u8]) -> Result<Hash64, String> {
    let mut value: serde_json::Value = serde_json::from_slice(body).map_err(|e| format!("the request body is not JSON: {e}"))?;
    let object = value.as_object_mut().ok_or("the request body is not a JSON object")?;
    object.remove("stream");
    object.remove("stream_options");
    let canonical = misaka_palw_constraint::canonical::to_rfc8785(&value)?;
    Ok(kaspa_hashes::blake2b_512_keyed(DOMAIN_REQUEST, &canonical))
}

/// What the table says about a key.
pub enum Begin<'a> {
    /// First sight (or a retry of a request that left nothing): run it, then [`Reservation::complete`].
    Fresh(Reservation<'a>),
    /// Finished before, same request: this is its response.
    Replay(serde_json::Value),
    /// The first attempt is still running.
    InProgress,
    /// The key names a different request.
    Conflict,
    /// The table is full of running requests (never evicted): ask later.
    Overloaded,
}

enum Entry {
    Running { digest: Hash64 },
    Done { digest: Hash64, response: serde_json::Value, stored_unix: u64 },
}

struct Inner {
    entries: HashMap<Hash64, Entry>,
    /// Finished keys, oldest first (the eviction order).
    done_order: VecDeque<Hash64>,
}

/// The table of keys, over a directory.
pub struct IdempotencyStore {
    dir: PathBuf,
    ttl_secs: u64,
    max_entries: usize,
    now: Box<dyn Fn() -> u64 + Send + Sync>,
    inner: Mutex<Inner>,
}

/// A running request's claim on its key: [`complete`](Self::complete) it with the response, or drop it and the key is free.
pub struct Reservation<'a> {
    store: &'a IdempotencyStore,
    key_id: Hash64,
    digest: Hash64,
    done: bool,
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl IdempotencyStore {
    /// The store over `<outbox>/idempotency`, with the system clock.
    pub fn open(outbox: &Path) -> Self {
        Self::with_clock(outbox.join("idempotency"), TTL_SECS, MAX_ENTRIES, Box::new(unix_now))
    }

    pub fn with_clock(dir: PathBuf, ttl_secs: u64, max_entries: usize, now: Box<dyn Fn() -> u64 + Send + Sync>) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let store = Self {
            dir,
            ttl_secs,
            max_entries: max_entries.max(1),
            now,
            inner: Mutex::new(Inner { entries: HashMap::new(), done_order: VecDeque::new() }),
        };
        store.sweep_disk();
        store
    }

    fn path_of(&self, key_id: &Hash64) -> PathBuf {
        self.dir.join(format!("idem-{}.json", &faster_hex::hex_string(key_id.as_byte_slice())[..40]))
    }

    /// Remove every record on disk past its time to live (a record that does not parse goes too).
    fn sweep_disk(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        let now = (self.now)();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("idem-") && n.ends_with(".json")) {
                continue;
            }
            let alive = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|doc| doc.get("stored_unix").and_then(serde_json::Value::as_u64))
                .is_some_and(|stored| now.saturating_sub(stored) < self.ttl_secs);
            if !alive {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// A finished record from disk, if one is there and alive and well-formed.
    fn load(&self, key_id: &Hash64) -> Option<Entry> {
        let path = self.path_of(key_id);
        let bytes = std::fs::read(&path).ok()?;
        let Ok(doc) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            let _ = std::fs::remove_file(&path);
            return None;
        };
        let well_formed = doc.get("schema").and_then(serde_json::Value::as_str) == Some(SCHEMA)
            && doc.get("key_id").and_then(serde_json::Value::as_str) == Some(faster_hex::hex_string(key_id.as_byte_slice()).as_str());
        let stored_unix = doc.get("stored_unix").and_then(serde_json::Value::as_u64);
        let digest = doc.get("request_digest").and_then(serde_json::Value::as_str).and_then(|h| {
            let mut out = [0u8; 64];
            (h.len() == 128 && faster_hex::hex_decode(h.as_bytes(), &mut out).is_ok()).then(|| Hash64::from_bytes(out))
        });
        let response = doc.get("response").cloned();
        match (well_formed, stored_unix, digest, response) {
            (true, Some(stored_unix), Some(digest), Some(response)) if (self.now)().saturating_sub(stored_unix) < self.ttl_secs => {
                Some(Entry::Done { digest, response, stored_unix })
            }
            _ => {
                let _ = std::fs::remove_file(&path);
                None
            }
        }
    }

    /// Look the key up and, when nothing stands in the way, reserve it.
    pub fn begin(&self, key: &IdempotencyKey, digest: Hash64) -> Begin<'_> {
        let key_id = key.id();
        let mut inner = self.inner.lock().expect("the idempotency lock is never poisoned");
        // A finished record that expired in memory is dropped like one on disk.
        if let Some(Entry::Done { stored_unix, .. }) = inner.entries.get(&key_id)
            && (self.now)().saturating_sub(*stored_unix) >= self.ttl_secs
        {
            inner.entries.remove(&key_id);
            inner.done_order.retain(|k| *k != key_id);
            let _ = std::fs::remove_file(self.path_of(&key_id));
        }
        if !inner.entries.contains_key(&key_id)
            && let Some(done) = self.load(&key_id)
        {
            // Cached only if there is room (finished records make room); otherwise answered from the disk copy and not held.
            if Self::make_room(&mut inner, self.max_entries) {
                inner.entries.insert(key_id, done);
                inner.done_order.push_back(key_id);
            } else if let Entry::Done { digest: stored, response, .. } = done {
                return if stored == digest { Begin::Replay(response) } else { Begin::Conflict };
            }
        }
        match inner.entries.get(&key_id) {
            Some(Entry::Running { digest: running }) => return if *running == digest { Begin::InProgress } else { Begin::Conflict },
            Some(Entry::Done { digest: stored, response, .. }) => {
                return if *stored == digest { Begin::Replay(response.clone()) } else { Begin::Conflict };
            }
            None => {}
        }
        // Room: evict the oldest finished record; a table of nothing but running requests refuses.
        if !Self::make_room(&mut inner, self.max_entries) {
            return Begin::Overloaded;
        }
        inner.entries.insert(key_id, Entry::Running { digest });
        Begin::Fresh(Reservation { store: self, key_id, digest, done: false })
    }

    /// Make space for one more record by evicting the oldest finished ones. `false`: the table is full of running requests.
    fn make_room(inner: &mut Inner, max_entries: usize) -> bool {
        while inner.entries.len() >= max_entries {
            let Some(oldest) = inner.done_order.pop_front() else { return false };
            if matches!(inner.entries.get(&oldest), Some(Entry::Done { .. })) {
                inner.entries.remove(&oldest);
            }
        }
        true
    }

    /// Records held in memory (running + finished).
    pub fn len(&self) -> usize {
        self.inner.lock().expect("the idempotency lock is never poisoned").entries.len()
    }
}

impl Reservation<'_> {
    /// The request finished: store its response under the key, in memory and on disk.
    pub fn complete(mut self, response: serde_json::Value) {
        let stored_unix = (self.store.now)();
        let doc = serde_json::json!({
            "schema": SCHEMA,
            "key_id": faster_hex::hex_string(self.key_id.as_byte_slice()),
            "request_digest": faster_hex::hex_string(self.digest.as_byte_slice()),
            "stored_unix": stored_unix,
            "response": response,
        });
        // Atomic: a reader (or a restart) never sees half a record. A disk failure is logged and the memory record stands —
        // the retry within this process still replays; across a restart it would run again, which is the cost of a full disk.
        let path = self.store.path_of(&self.key_id);
        let tmp = path.with_extension("json.tmp");
        let written = serde_json::to_vec(&doc)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(&tmp, bytes).map_err(|e| e.to_string()))
            .and_then(|()| std::fs::rename(&tmp, &path).map_err(|e| e.to_string()));
        if let Err(e) = written {
            eprintln!("[misaka-palw-gateway] cannot persist an idempotency record ({e}); it will not survive a restart");
            let _ = std::fs::remove_file(&tmp);
        }
        let mut inner = self.store.inner.lock().expect("the idempotency lock is never poisoned");
        inner.entries.insert(self.key_id, Entry::Done { digest: self.digest, response, stored_unix });
        inner.done_order.push_back(self.key_id);
        self.done = true;
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.done {
            // Failed, refused or cancelled: nothing was committed, so the key is free again.
            let mut inner = self.store.inner.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(inner.entries.get(&self.key_id), Some(Entry::Running { .. })) {
                inner.entries.remove(&self.key_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("misaka-gw-idem-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn store_at(d: &Path, clock: &Arc<AtomicU64>, max: usize) -> IdempotencyStore {
        let c = Arc::clone(clock);
        IdempotencyStore::with_clock(d.to_path_buf(), 100, max, Box::new(move || c.load(Ordering::Relaxed)))
    }

    fn key(s: &str) -> IdempotencyKey {
        IdempotencyKey::parse(s).expect("a valid key")
    }

    fn req(extra: &str) -> Hash64 {
        request_digest(format!(r#"{{"messages":[{{"role":"user","content":"hi"}}]{extra}}}"#).as_bytes()).unwrap()
    }

    #[test]
    fn keys_are_bounded_printable_ascii() {
        assert!(IdempotencyKey::parse("3f8b1c7e-2a0d-4a55-9c1e-0f6a7b2c9d10").is_ok());
        assert!(IdempotencyKey::parse("").is_err());
        assert!(IdempotencyKey::parse("has space").is_err());
        assert!(IdempotencyKey::parse("tab\there").is_err());
        assert!(IdempotencyKey::parse("日本語").is_err());
        assert!(IdempotencyKey::parse(&"k".repeat(MAX_KEY_BYTES)).is_ok());
        assert!(IdempotencyKey::parse(&"k".repeat(MAX_KEY_BYTES + 1)).is_err());
        assert_ne!(key("a").id(), key("b").id());
    }

    /// **The canonical request**: transport members and layout do not matter; every semantic member does.
    #[test]
    fn the_digest_ignores_delivery_and_layout_and_notices_everything_else() {
        let base = req("");
        assert_eq!(base, req(r#","stream":true"#), "streaming is delivery, not the request");
        assert_eq!(base, req(r#","stream":false,"stream_options":{"include_usage":true}"#));
        let reordered = request_digest(br#"{ "messages" : [ { "content":"hi", "role":"user" } ] }"#).unwrap();
        assert_eq!(base, reordered, "key order and whitespace are not the request");
        for (what, extra) in [
            ("temperature", r#","temperature":0.5"#),
            ("max_tokens", r#","max_tokens":7"#),
            ("seed", r#","seed":"00""#),
            ("model", r#","model":"x""#),
            ("stop", r#","stop":["a"]"#),
            ("derive", r#","derive":"scene""#),
            ("tools", r#","tools":[]"#),
        ] {
            assert_ne!(base, req(extra), "{what} changes the request");
        }
        let other_text = request_digest(br#"{"messages":[{"role":"user","content":"ho"}]}"#).unwrap();
        assert_ne!(base, other_text, "one changed letter of the user's message is another request");
        assert!(request_digest(b"[1]").is_err() && request_digest(b"not json").is_err());
    }

    #[test]
    fn the_same_key_and_request_replay_the_stored_response_and_a_different_request_is_refused() {
        let d = dir("replay");
        let clock = Arc::new(AtomicU64::new(1_000));
        let store = store_at(&d, &clock, 8);
        let k = key("k-1");
        let digest = req("");
        let Begin::Fresh(reservation) = store.begin(&k, digest) else { panic!("first sight runs") };
        // While it runs, a duplicate is told so; a different request under the key is a conflict.
        assert!(matches!(store.begin(&k, digest), Begin::InProgress));
        assert!(matches!(store.begin(&k, req(r#","max_tokens":3"#)), Begin::Conflict));
        reservation.complete(serde_json::json!({ "misaka": { "fp_claim_id": "aa" } }));
        match store.begin(&k, digest) {
            Begin::Replay(body) => assert_eq!(body["misaka"]["fp_claim_id"], "aa", "the stored response, not a new job"),
            _ => panic!("a finished request replays"),
        }
        assert!(matches!(store.begin(&k, req(r#","max_tokens":3"#)), Begin::Conflict), "the key names a different request");
        // Another key is independent.
        assert!(matches!(store.begin(&key("k-2"), digest), Begin::Fresh(_)));
    }

    #[test]
    fn a_failed_or_cancelled_request_leaves_nothing_behind_so_its_retry_runs() {
        let d = dir("abandon");
        let clock = Arc::new(AtomicU64::new(1_000));
        let store = store_at(&d, &clock, 8);
        let k = key("k");
        let digest = req("");
        match store.begin(&k, digest) {
            Begin::Fresh(reservation) => drop(reservation),
            _ => panic!(),
        }
        assert_eq!(store.len(), 0, "an abandoned reservation is gone");
        assert!(matches!(store.begin(&k, digest), Begin::Fresh(_)), "the retry runs");
    }

    #[test]
    fn a_finished_record_survives_a_restart_and_the_key_is_never_written_down() {
        let d = dir("restart");
        let clock = Arc::new(AtomicU64::new(1_000));
        let digest = req("");
        {
            let store = store_at(&d, &clock, 8);
            let Begin::Fresh(r) = store.begin(&key("secret-token-123"), digest) else { panic!() };
            r.complete(serde_json::json!({ "id": "palwcmpl-1" }));
        }
        let files: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(files[0].path()).unwrap();
        assert!(!text.contains("secret-token-123"), "only a hash of the key is on disk: {text}");
        // A new process over the same directory.
        let store = store_at(&d, &clock, 8);
        match store.begin(&key("secret-token-123"), digest) {
            Begin::Replay(body) => assert_eq!(body["id"], "palwcmpl-1"),
            _ => panic!("the retry after a restart still finds its answer"),
        }
        assert!(matches!(store.begin(&key("secret-token-123"), req(r#","seed":"1""#)), Begin::Conflict));
    }

    #[test]
    fn a_record_expires_and_a_corrupt_one_is_ignored() {
        let d = dir("ttl");
        let clock = Arc::new(AtomicU64::new(1_000));
        let digest = req("");
        let store = store_at(&d, &clock, 8);
        let Begin::Fresh(r) = store.begin(&key("k"), digest) else { panic!() };
        r.complete(serde_json::json!({ "id": "x" }));
        clock.store(1_099, Ordering::Relaxed);
        assert!(matches!(store.begin(&key("k"), digest), Begin::Replay(_)), "alive just inside the ttl");
        clock.store(1_100, Ordering::Relaxed);
        assert!(matches!(store.begin(&key("k"), digest), Begin::Fresh(_)), "expired at the ttl: the request runs again");
        // A corrupt file under another key's name is not a record.
        let store2 = store_at(&d, &clock, 8);
        let path = store2.path_of(&key("junk").id());
        std::fs::write(&path, b"{ not json").unwrap();
        assert!(matches!(store2.begin(&key("junk"), digest), Begin::Fresh(_)));
        assert!(!path.exists(), "the corrupt record was removed");
    }

    #[test]
    fn the_table_is_bounded_it_evicts_finished_records_and_never_a_running_one() {
        let d = dir("bound");
        let clock = Arc::new(AtomicU64::new(1_000));
        let store = store_at(&d, &clock, 2);
        let digest = req("");
        let Begin::Fresh(a) = store.begin(&key("a"), digest) else { panic!() };
        a.complete(serde_json::json!({ "id": "a" }));
        let Begin::Fresh(running) = store.begin(&key("b"), digest) else { panic!() };
        // Full (one finished, one running): the finished one is evicted for a newcomer.
        let Begin::Fresh(c) = store.begin(&key("c"), digest) else { panic!("room by evicting the finished record") };
        assert_eq!(store.len(), 2);
        // Now both are running: nothing to evict.
        assert!(matches!(store.begin(&key("d"), digest), Begin::Overloaded));
        drop((running, c));
        // `a` was evicted from memory but its disk record still answers.
        assert!(matches!(store.begin(&key("a"), digest), Begin::Replay(_)), "the disk is authoritative");
    }

    #[test]
    fn concurrent_duplicates_run_exactly_once() {
        let d = dir("race");
        let clock = Arc::new(AtomicU64::new(1_000));
        let store = Arc::new(store_at(&d, &clock, 64));
        let digest = req("");
        let fresh = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let (store, fresh) = (Arc::clone(&store), Arc::clone(&fresh));
            handles.push(std::thread::spawn(move || {
                if let Begin::Fresh(r) = store.begin(&key("same"), digest) {
                    fresh.fetch_add(1, Ordering::AcqRel);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    r.complete(serde_json::json!({ "id": "once" }));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(fresh.load(Ordering::Acquire), 1, "sixteen duplicates, one inference");
    }
}
