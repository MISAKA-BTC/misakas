//! **The chat path end to end, in process** (RFC-0001 §2.7): a real listener, the real `serve_connection`, admission, `prepare_request`,
//! `handle_chat`, the caller-side bindings, the outbox, the idempotency table and the status route — against an in-process worker over
//! the real BASE-0 floor engine ([`crate::testkit::FloorWorker`]). Nothing here is mocked above the worker's frame boundary.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kaspa_consensus_core::palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use misaka_palw::host_security::ConfinementBackend;

use crate::testkit::{FloorWorker, identity, offline_source, temp_dir};
use crate::{Config, Identity, JobRunner, PublicJobBudget, Services, SourceRates, chain, pool};

pub(crate) struct Harness {
    pub dir: PathBuf,
    pub config: Config,
    pub identity: Identity,
    pub worker: Arc<FloorWorker>,
    pub source: chain::ChainSource,
    pub services: Services,
    pub in_flight: AtomicUsize,
    pub budget: Mutex<PublicJobBudget>,
    pub sources: Mutex<SourceRates>,
    pub gate: pool::SourceGate,
}

impl Harness {
    pub fn new(name: &str) -> Arc<Self> {
        let dir = temp_dir(name);
        let worker = Arc::new(FloorWorker::new(&dir.join("traces"), PalwPromptIdsFormV1::Flat));
        let mut config = crate::testkit::config(&dir);
        // The anchor-file source knows nothing of the bond's room, so the operator declares it (the offline form).
        config.bond_exposure_room_sompi = 1_000_000_000;
        config.claim_exposure_sompi = 1_000;
        let identity = identity(&worker.profile);
        let source = offline_source(&dir);
        let services = Services::open(&dir);
        Arc::new(Self {
            dir,
            config,
            identity,
            worker,
            source,
            services,
            in_flight: AtomicUsize::new(0),
            budget: Mutex::new(PublicJobBudget::new()),
            sources: Mutex::new(SourceRates::default()),
            gate: pool::SourceGate::new(8, 4),
        })
    }

    /// One connection, served by the real `serve_connection` on a thread. Returns the client end and the server thread.
    pub fn connect(self: &Arc<Self>) -> (TcpStream, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let me = Arc::clone(self);
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            crate::serve_connection(
                &mut stream,
                &me.config,
                &me.identity,
                &*me.worker,
                &me.source,
                &me.in_flight,
                &me.budget,
                &me.sources,
                ConfinementBackend::None,
                false,
                &me.gate,
                &me.services,
            );
        });
        let client = TcpStream::connect(addr).expect("connect");
        client.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        (client, server)
    }

    /// Send one request and read the whole response.
    pub fn call(self: &Arc<Self>, raw: &[u8]) -> Response {
        let (mut client, server) = self.connect();
        client.write_all(raw).unwrap();
        let response = Response::read(&mut client);
        server.join().expect("the server thread");
        response
    }

    pub fn outbox_files(&self, suffix: &str) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("fp-job-") && n.ends_with(suffix)))
            .collect();
        out.sort();
        out
    }

    pub fn committed_claims(&self) -> usize {
        self.outbox_files(".commitment-unsigned.borsh").len()
    }

    pub fn traces(&self) -> usize {
        std::fs::read_dir(self.dir.join("traces")).map(|d| d.flatten().count()).unwrap_or(0)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(crate) struct Response {
    pub head: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn read(client: &mut TcpStream) -> Self {
        let mut bytes = Vec::new();
        let _ = client.read_to_end(&mut bytes);
        let split = bytes.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4).unwrap_or(bytes.len());
        Self { head: String::from_utf8_lossy(&bytes[..split]).into_owned(), body: bytes[split..].to_vec() }
    }

    pub fn status(&self) -> u16 {
        self.head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|l| {
            let (n, v) = l.split_once(':')?;
            n.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&self.body)))
    }

    /// The `data:` events of an SSE body, parsed (the `[DONE]` sentinel excluded).
    pub fn events(&self) -> Vec<serde_json::Value> {
        String::from_utf8_lossy(&self.body)
            .lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .filter(|d| *d != "[DONE]")
            .map(|d| serde_json::from_str(d).expect("an SSE event is JSON"))
            .collect()
    }
}

pub(crate) fn post(body: &serde_json::Value, key: Option<&str>) -> Vec<u8> {
    let body = serde_json::to_vec(body).unwrap();
    let mut head = format!("POST /v1/chat/completions HTTP/1.1\r\nHost: t\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", body.len());
    if let Some(key) = key {
        head.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    head.push_str("\r\n");
    let mut out = head.into_bytes();
    out.extend_from_slice(&body);
    out
}

pub(crate) fn get(path: &str) -> Vec<u8> {
    format!("GET {path} HTTP/1.1\r\nHost: t\r\n\r\n").into_bytes()
}

pub(crate) fn chat(content: &str) -> serde_json::Value {
    serde_json::json!({ "messages": [{ "role": "user", "content": content }], "max_tokens": 4 })
}

fn claim_of(body: &serde_json::Value) -> String {
    body["misaka"]["fp_claim_id"].as_str().expect("a claim id").to_string()
}

#[test]
fn a_chat_request_becomes_a_queued_commitment_and_says_it_is_not_final() {
    let h = Harness::new("e2e-basic");
    let response = h.call(&post(&chat("hi"), None));
    assert_eq!(response.status(), 200, "{}", String::from_utf8_lossy(&response.body));
    let body = response.json();
    assert_eq!(body["misaka"]["committed"], true);
    assert_eq!(h.worker.runs(), 1);
    // The status the response carries: committed, and NOT final.
    assert_eq!(body["misaka"]["request"]["status"], "committed");
    assert_eq!(body["misaka"]["request"]["final"], false);
    // The outbox: the framed result, the unsigned commitment and the summary — and the commitment's claim id is the response's.
    let commitments = h.outbox_files(".commitment-unsigned.borsh");
    assert_eq!(commitments.len(), 1);
    let commitment: PalwFreePromptCommitmentV3 = borsh::from_slice(&std::fs::read(&commitments[0]).unwrap()).unwrap();
    assert_eq!(faster_hex::hex_string(fp_claim_id_v3(&commitment).as_byte_slice()), claim_of(&body));
    assert_eq!(h.outbox_files(".result.borsh").len(), 1);
    assert_eq!(h.traces(), 1, "the worker's retained trace is where the gateway looks for it");
}

/// **Idempotency**: the retry of a finished request is the same claim, with no second inference, no second commitment and no charge.
#[test]
fn a_retry_with_the_same_key_replays_the_same_claim_and_runs_no_second_inference() {
    let h = Harness::new("e2e-idem");
    let request = post(&chat("hello"), Some("retry-key-1"));
    let first = h.call(&request);
    assert_eq!(first.status(), 200);
    let first_body = first.json();
    let spent = h.budget.lock().unwrap().spent_sompi;
    assert_eq!((h.worker.runs(), h.committed_claims()), (1, 1));

    let second = h.call(&request);
    assert_eq!(second.status(), 200);
    assert_eq!(second.header("idempotent-replayed").as_deref(), Some("true"));
    let second_body = second.json();
    assert_eq!(claim_of(&second_body), claim_of(&first_body), "the same claim, not a second one");
    assert_eq!(second_body["choices"][0]["message"]["content"], first_body["choices"][0]["message"]["content"]);
    assert_eq!(second_body["misaka"]["idempotent_replay"], true);
    assert_eq!(h.worker.runs(), 1, "no second inference");
    assert_eq!(h.committed_claims(), 1, "no second commitment");
    assert_eq!(h.budget.lock().unwrap().spent_sompi, spent, "no second charge against the public-job budget");

    // Without a key the same request IS a second job — which is exactly the retry hazard the key removes.
    let keyless = h.call(&post(&chat("hello"), None));
    assert_eq!(keyless.status(), 200);
    assert_eq!(h.worker.runs(), 2);
    assert_ne!(claim_of(&keyless.json()), claim_of(&first_body), "a fresh job_nonce is a fresh claim id for the same work");
}

#[test]
fn the_same_key_for_a_different_request_is_refused_and_runs_nothing() {
    let h = Harness::new("e2e-idem-conflict");
    assert_eq!(h.call(&post(&chat("one"), Some("k"))).status(), 200);
    let refused = h.call(&post(&chat("two"), Some("k")));
    assert_eq!(refused.status(), 409, "{}", String::from_utf8_lossy(&refused.body));
    assert!(refused.json()["error"]["message"].as_str().unwrap().contains("different request"));
    assert_eq!((h.worker.runs(), h.committed_claims()), (1, 1));
    // A malformed key is a 400 before anything runs.
    let bad = h.call(&post(&chat("one"), Some("has space")));
    assert_eq!(bad.status(), 400);
    assert_eq!(h.worker.runs(), 1);
}

#[test]
fn a_streaming_retry_of_a_finished_request_is_replayed_as_a_stream_without_a_second_inference() {
    let h = Harness::new("e2e-idem-sse");
    let mut streaming = chat("again");
    let first = h.call(&post(&chat("again"), Some("sse-key")));
    let first_claim = claim_of(&first.json());
    streaming["stream"] = serde_json::json!(true);
    let replay = h.call(&post(&streaming, Some("sse-key")));
    assert_eq!(replay.status(), 200);
    assert!(replay.head.to_ascii_lowercase().contains("text/event-stream"), "{}", replay.head);
    let events = replay.events();
    let terminal = events.iter().find_map(|e| e.get("misaka").filter(|m| m.get("fp_claim_id").is_some())).expect("a terminal event with the claim");
    assert_eq!(terminal["fp_claim_id"], first_claim.as_str());
    assert_eq!(h.worker.runs(), 1, "a retry in the other delivery mode is still the same inference");
}

/// **Status honesty over a stream**: every delta says `streaming` and not final; the terminal event says what became of it.
#[test]
fn every_streamed_chunk_says_streaming_and_the_terminal_event_says_committed_and_not_final() {
    let h = Harness::new("e2e-sse-status");
    // The status route is reachable WHILE the run is in flight: the hook asks the book from inside the worker.
    let seen_streaming = Arc::new(AtomicUsize::new(0));
    {
        let (h2, seen) = (Arc::clone(&h), Arc::clone(&seen_streaming));
        h.worker.set_hook(move |_| {
            if h2.services.book.streaming_count() == 1 {
                seen.fetch_add(1, Ordering::AcqRel);
            }
        });
    }
    let mut body = chat("stream");
    body["stream"] = serde_json::json!(true);
    let response = h.call(&post(&body, None));
    assert_eq!(response.status(), 200);
    let events = response.events();
    assert!(!events.is_empty(), "{}{}", response.head, String::from_utf8_lossy(&response.body));
    let deltas: Vec<_> = events.iter().filter(|e| e["choices"].as_array().is_some_and(|c| !c.is_empty())).collect();
    assert!(!deltas.is_empty());
    for e in &deltas {
        assert_eq!(e["misaka"]["status"], "streaming", "{e}");
        assert_eq!(e["misaka"]["final"], false);
    }
    assert!(seen_streaming.load(Ordering::Acquire) > 0, "the request was `streaming` in the book while it ran");
    assert_eq!(h.services.book.streaming_count(), 0, "and it left the book when it ended");
    let terminal = events.iter().find_map(|e| e.get("misaka").filter(|m| m.get("request").is_some())).expect("terminal event");
    assert_eq!(terminal["request"]["status"], "committed");
    assert_eq!(terminal["request"]["final"], false);
}

/// **Cancellation**: a client that leaves before the answer is committed leaves nothing behind.
#[test]
fn a_client_that_disconnects_mid_run_is_never_committed_and_its_key_is_free_again() {
    let h = Harness::new("e2e-cancel");
    let (mut client, server) = h.connect();
    // The hook runs on the server thread after the second streamed token: the client closes its socket right there.
    let victim = Arc::new(Mutex::new(Some(client.try_clone().unwrap())));
    {
        let victim = Arc::clone(&victim);
        h.worker.set_hook(move |index| {
            if index == 1
                && let Some(socket) = victim.lock().unwrap().take()
            {
                let _ = socket.shutdown(std::net::Shutdown::Both);
            }
        });
    }
    client.write_all(&post(&chat("leave"), Some("cancel-key"))).unwrap();
    server.join().expect("the server thread returns when its client is gone");
    drop(client);
    assert_eq!(h.worker.runs(), 1, "the run was started (it was already on the worker) and drained");
    assert_eq!(h.committed_claims(), 0, "NO commitment is written for a cancelled request");
    assert!(h.outbox_files(".result.borsh").is_empty() && h.outbox_files(".json").is_empty(), "no outbox artifact of any kind");
    assert_eq!(h.traces(), 0, "the retained trace of a claim that will never exist is removed");
    assert_eq!(h.budget.lock().unwrap().spent_sompi, 0, "and nothing was charged against the public-job budget");
    assert_eq!(h.budget.lock().unwrap().committed_jobs, 0);
    assert_eq!(h.in_flight.load(Ordering::Acquire), 0, "the queue place was released");
    assert_eq!(h.gate.tracked_sources(), 0, "and so was the source's share of in-flight jobs");
    h.worker.set_hook(|_| {});
    // The key was released with the request: the retry runs, commits, and is a normal first answer.
    let retry = h.call(&post(&chat("leave"), Some("cancel-key")));
    assert_eq!(retry.status(), 200);
    assert_eq!(retry.json()["misaka"]["committed"], true);
    assert_eq!((h.worker.runs(), h.committed_claims()), (2, 1));
}

#[test]
fn a_streaming_client_that_hangs_up_is_cancelled_too_and_the_book_remembers_it() {
    let h = Harness::new("e2e-cancel-sse");
    let (mut client, server) = h.connect();
    let victim = Arc::new(Mutex::new(Some(client.try_clone().unwrap())));
    {
        let victim = Arc::clone(&victim);
        h.worker.set_hook(move |index| {
            if index == 0
                && let Some(socket) = victim.lock().unwrap().take()
            {
                let _ = socket.shutdown(std::net::Shutdown::Both);
            }
        });
    }
    let mut body = chat("hang up");
    body["stream"] = serde_json::json!(true);
    client.write_all(&post(&body, None)).unwrap();
    server.join().unwrap();
    assert_eq!(h.committed_claims(), 0);
    assert_eq!(h.traces(), 0);
    assert_eq!(h.services.book.streaming_count(), 0);
    assert_eq!(h.services.book.cancelled_count(), 1, "the request is remembered as cancelled, not as failed");
}

/// A request still waiting for the worker slot when its client leaves is never run.
#[test]
fn a_queued_request_whose_client_left_is_not_run_at_all() {
    let h = Harness::new("e2e-cancel-queued");
    // Request A holds the slot: its hook blocks until the test releases it.
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let started_tx = Mutex::new(started_tx);
    h.worker.set_hook(move |index| {
        if index == 0 {
            let _ = started_tx.lock().unwrap().send(());
            let _ = release_rx.lock().unwrap().recv_timeout(Duration::from_secs(20));
        }
    });
    let (mut a, server_a) = h.connect();
    a.write_all(&post(&chat("holder"), None)).unwrap();
    started_rx.recv_timeout(Duration::from_secs(20)).expect("A is on the worker");
    // Request B queues behind it, and its client leaves while it waits.
    let (mut b, server_b) = h.connect();
    b.write_all(&post(&chat("queued"), None)).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(JobRunner::waiting(&*h.worker) >= 1, "B is waiting for the slot");
    drop(b);
    std::thread::sleep(Duration::from_millis(100));
    release_tx.send(()).unwrap();
    let a_response = Response::read(&mut a);
    server_a.join().unwrap();
    server_b.join().unwrap();
    assert_eq!(a_response.status(), 200);
    assert_eq!(h.worker.runs(), 1, "B never reached the engine");
    assert_eq!(h.committed_claims(), 1, "only A committed");
}

#[test]
fn a_full_queue_answers_503_with_a_retry_after_and_runs_nothing() {
    let h = Harness::new("e2e-503");
    h.in_flight.store(crate::in_flight_cap(1), Ordering::Release);
    let response = h.call(&post(&chat("late"), None));
    assert_eq!(response.status(), 503, "{}", response.head);
    assert!(response.header("retry-after").is_some_and(|v| v.parse::<u32>().is_ok_and(|n| n > 0)), "the 503 names when to retry: {}", response.head);
    assert_eq!(h.worker.runs(), 0);
    assert_eq!(h.in_flight.load(Ordering::Acquire), crate::in_flight_cap(1), "a refusal reserves nothing and releases nothing");
}

/// The status route follows a request from the outbox: committed, then submitted once the rail has recorded a submission.
#[test]
fn the_status_route_reports_committed_then_submitted_and_never_final_without_a_chain_fact() {
    let h = Harness::new("e2e-status");
    let body = h.call(&post(&chat("track"), None)).json();
    let id = body["id"].as_str().unwrap().to_string();
    let first_status = h.call(&get(&format!("/v1/requests/{id}")));
    assert_eq!(first_status.json()["status"], "committed", "{} / {}", first_status.head, String::from_utf8_lossy(&first_status.body));
    // The rail submits: its record lands beside the gateway's.
    let stem = crate::status::stem_of_completion_id(&id).unwrap();
    std::fs::write(h.dir.join(format!("{stem}.rail.json")), serde_json::json!({ "submitted": "ab".repeat(64), "relayed": null }).to_string()).unwrap();
    let after = h.call(&get(&format!("/v1/requests/{id}"))).json();
    assert_eq!(after["status"], "submitted");
    assert_eq!(after["final"], false, "a rail record is not a chain fact");
    assert_eq!(after["claim_id"], claim_of(&body).as_str());
    assert_eq!(after["submission_txid"], "ab".repeat(64).as_str());
    // An id this gateway never issued, and a path trying to leave the outbox.
    assert_eq!(h.call(&get("/v1/requests/palwcmpl-000000000000000000000000")).status(), 404);
    assert_eq!(h.call(&get("/v1/requests/..%2F..%2Fetc%2Fpasswd")).status(), 404);
    // The replay of a finished request recomputes its status NOW.
    let keyed = h.call(&post(&chat("keyed"), Some("track-key"))).json();
    let replay_raw = h.call(&post(&chat("keyed"), Some("track-key")));
    assert_eq!(replay_raw.status(), 200, "{}{}", replay_raw.head, String::from_utf8_lossy(&replay_raw.body));
    let replayed = replay_raw.json();
    assert_eq!(replayed["misaka"]["request"]["status"], "committed");
    assert_eq!(replayed["misaka"]["request"]["claim_id"], claim_of(&keyed).as_str());
}
