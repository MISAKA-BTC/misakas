//! **What became of a request: `streaming → committed → submitted → final | voided`, and never a word more than is known.**
//!
//! An answer on a screen and a claim on a chain are different facts, and the first is available long before the second
//! exists. This module is the one place the gateway says which of them a client is looking at:
//!
//! | status | what is true | who says so |
//! |---|---|---|
//! | `streaming` | tokens are being shown; **no commitment exists**; nothing shown is yet a result | this process, in flight |
//! | `answered` | the run ended and was NOT committed (the budget, the chain's facts, `--answer-never-commit`); no claim exists | the outbox summary |
//! | `committed` | the commitment is queued in the outbox, its claim id fixed; it is not on any chain | the outbox |
//! | `submitted` | the rail (or a node's pool) holds it: relayed, included, licensed, in its challenge window — **reversible** | the rail record and the chain |
//! | `final` | the chain's claim row is `Final` and `finality_depth` DAA deep | the chain, **labelled** |
//! | `voided` | the chain's claim row is voided and settled | the chain, **labelled** |
//! | `cancelled` | the client left before the commitment was written; nothing was committed | this process |
//! | `misattributed` | the chain's row for this claim names ANOTHER executor bond | the chain, **labelled** |
//!
//! **`final` and `voided` come from chain facts only**, and a fact read from a node is `UNVERIFIED_REMOTE_STATE` unless a
//! state proof against a pinned header says otherwise (RFC-0009 §6; `misaka_palw_remote::trust`). The label is part of
//! the answer, not a footnote: a node that lies, or two that agree and are wrong, can make a request look `final`, and the
//! client is told exactly that. No code path here derives `final` from the gateway's own files.

use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Mutex;

use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_remote::track::{ChainObservation, ClaimTracker, TrackState};
use misaka_palw_remote::trust::Provenance;

/// The default `finality_depth`: the rail's own (`--finality-depth`), so `final` here and `Final` there are one statement.
pub const DEFAULT_FINALITY_DEPTH: u64 = 60;
/// The completion id's prefix; the id is `palwcmpl-` + the first 24 hex characters of the job id.
pub const COMPLETION_PREFIX: &str = "palwcmpl-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    Streaming,
    Answered,
    Committed,
    Submitted,
    Final,
    Voided,
    Cancelled,
    Misattributed,
}

impl RequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Answered => "answered",
            Self::Committed => "committed",
            Self::Submitted => "submitted",
            Self::Final => "final",
            Self::Voided => "voided",
            Self::Cancelled => "cancelled",
            Self::Misattributed => "misattributed",
        }
    }

    /// Only the chain's `Final` is final. Every other status — including `committed` and `submitted` — is not a result.
    pub fn is_final(self) -> bool {
        self == Self::Final
    }
}

/// What the outbox says of a job (`fp-job-<16 hex>.json`, the rail's `.rail.json`, the retired-commitment marker).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalFacts {
    pub committed: bool,
    pub claim_id: Option<Hash64>,
    pub job_id: Option<Hash64>,
    pub output_root: Option<Hash64>,
    pub executor_bond: Option<TransactionOutpoint>,
    pub not_committed_because: Option<String>,
    /// The commitment's anchor lapsed and the gateway retired it (`.expired`): it will never be submitted.
    pub expired: bool,
    /// The rail's record: the txid it submitted, or `None`.
    pub rail_txid: Option<String>,
    /// The rail relayed it to at least one node.
    pub rail_relayed: bool,
}

fn parse_hash(hex: &str) -> Option<Hash64> {
    let mut out = [0u8; 64];
    (hex.len() == 128 && faster_hex::hex_decode(hex.as_bytes(), &mut out).is_ok()).then(|| Hash64::from_bytes(out))
}

fn parse_outpoint(text: &str) -> Option<TransactionOutpoint> {
    let (txid, index) = text.split_once(':')?;
    Some(TransactionOutpoint::new(parse_hash(txid)?, index.parse().ok()?))
}

/// The stem of a completion id: `palwcmpl-<24 hex>` → `fp-job-<16 hex>` (the gateway names a job's files by the first 16
/// hex characters of its id). `None` for anything else, so a request path can never name a file outside the outbox.
pub fn stem_of_completion_id(id: &str) -> Option<String> {
    let hex = id.strip_prefix(COMPLETION_PREFIX)?;
    (hex.len() == 24 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| format!("fp-job-{}", hex[..16].to_ascii_lowercase()))
}

/// Read a job's facts from the outbox by direct path probes (never a directory walk: SA-4).
pub fn read_local_facts(outbox: &Path, stem: &str) -> Option<LocalFacts> {
    let summary: serde_json::Value = serde_json::from_slice(&std::fs::read(outbox.join(format!("{stem}.json"))).ok()?).ok()?;
    let text = |k: &str| summary.get(k).and_then(serde_json::Value::as_str);
    let mut facts = LocalFacts {
        committed: summary.get("committed").and_then(serde_json::Value::as_bool).unwrap_or(false),
        claim_id: text("fp_claim_id").and_then(parse_hash),
        job_id: text("fp_job_id").and_then(parse_hash),
        output_root: text("output_root").and_then(parse_hash),
        executor_bond: summary.get("executor_bond").and_then(serde_json::Value::as_str).and_then(parse_outpoint),
        not_committed_because: text("not_committed_because").map(str::to_string),
        expired: outbox.join(format!("{stem}.commitment-unsigned.borsh{}", misaka_palw_fp_submit::EXPIRED_SUFFIX)).exists(),
        ..LocalFacts::default()
    };
    if let Some(rail) = std::fs::read(outbox.join(format!("{stem}.rail.json"))).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) {
        facts.rail_txid = rail.get("submitted").and_then(serde_json::Value::as_str).map(str::to_string);
        facts.rail_relayed = rail.get("relayed").is_some_and(|r| !r.is_null());
    }
    Some(facts)
}

/// A chain sighting of the claim, and how much it is worth.
#[derive(Clone, Debug)]
pub struct ChainSighting {
    pub state: TrackState,
    pub provenance: Provenance,
}

/// The gateway's reading of the chain's claim row. The production adapter is `chain::RpcChainSource`; tests supply a closure.
pub trait ClaimObserver: Sync {
    /// One poll: what the selected chain says of `claim_id` now, and how many independent nodes said it.
    fn observe(&self, claim_id: Hash64, tx_id: Option<Hash64>) -> Result<(ChainObservation, usize), String>;
}

/// What a client is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusReport {
    pub status: RequestStatus,
    /// The rail/chain tracker state behind `submitted`/`final`/`voided`, by name (`Relayed`, `Included`, `FinalPending`, …).
    pub chain_state: Option<String>,
    /// Where the chain facts came from — `UNVERIFIED_REMOTE_STATE (…)` or the proof; `None` when no chain fact was used.
    pub provenance: Option<String>,
    pub note: String,
}

impl StatusReport {
    pub fn is_final(&self) -> bool {
        self.status.is_final()
    }

    pub fn to_json(&self, local: &LocalFacts) -> serde_json::Value {
        serde_json::json!({
            "object": "misaka.request",
            "status": self.status.as_str(),
            // Only the chain's Final is final; the field exists so a client never has to infer it from the word.
            "final": self.status.is_final(),
            "claim_id": local.claim_id.map(|h| faster_hex::hex_string(h.as_byte_slice())),
            "job_id": local.job_id.map(|h| faster_hex::hex_string(h.as_byte_slice())),
            "output_root": local.output_root.map(|h| faster_hex::hex_string(h.as_byte_slice())),
            "submission_txid": local.rail_txid,
            "chain_state": self.chain_state,
            "provenance": self.provenance,
            "note": self.note,
        })
    }
}

fn report(status: RequestStatus, chain_state: Option<String>, provenance: Option<&Provenance>, note: &str) -> StatusReport {
    StatusReport { status, chain_state, provenance: provenance.map(Provenance::label), note: note.to_string() }
}

/// **The status of a finished job**, from the outbox and (when a node was asked) the chain.
///
/// `final` and `voided` are returned ONLY from a chain sighting whose tracker state is settled (`Final`/`Void`, i.e.
/// `finality_depth` DAA deep); a `FinalPending` claim is `submitted` with its chain state named, because it can still be
/// reorged out. With no sighting the best answer is what the outbox and the rail know.
pub fn derive_status(local: &LocalFacts, chain: Option<&ChainSighting>) -> StatusReport {
    if !local.committed {
        return report(
            RequestStatus::Answered,
            None,
            None,
            "answered without a claim: nothing was committed, so there is no chain fact to wait for",
        );
    }
    if let Some(sighting) = chain {
        let name = format!("{:?}", sighting.state).split([' ', '{']).next().unwrap_or("").to_string();
        let p = Some(&sighting.provenance);
        return match &sighting.state {
            TrackState::Final { .. } => report(
                RequestStatus::Final,
                Some(name),
                p,
                "the chain's claim row is Final and finality_depth DAA deep; the label says how this was learned",
            ),
            TrackState::Void { .. } => report(RequestStatus::Voided, Some(name), p, "the chain voided the claim; the answer is not a result"),
            TrackState::Misattributed { .. } => report(
                RequestStatus::Misattributed,
                Some(name),
                p,
                "the chain's claim row names a different executor bond: this claim is not this gateway's",
            ),
            TrackState::Built if local.rail_txid.is_none() && !local.rail_relayed => report(
                RequestStatus::Committed,
                Some(name),
                p,
                "queued in the outbox; no node holds it yet — it is not on chain until the rail submits it",
            ),
            _ => report(
                RequestStatus::Submitted,
                Some(name),
                p,
                "a node holds the claim, but it is not final: a pool entry, an inclusion, a licence or a challenge window can all be reversed",
            ),
        };
    }
    if local.rail_txid.is_some() || local.rail_relayed {
        return report(
            RequestStatus::Submitted,
            None,
            None,
            "the rail reports it submitted; no chain fact was read for this answer, so nothing further is claimed",
        );
    }
    if local.expired {
        return report(
            RequestStatus::Committed,
            None,
            None,
            "the commitment's anchor lapsed and it was retired (.expired): it will never be submitted",
        );
    }
    report(RequestStatus::Committed, None, None, "queued in the outbox; it is not on any chain until the rail submits it")
}

/// Ask `observer` about the claim and fold the answer through the rail's own tracker, so `final` here and `Final` in
/// `misaka-palw-fp-rail --track` are one definition. An observer error is not a status: the report falls back to what the
/// outbox knows and says the chain could not be read.
pub fn status_with_chain(
    local: &LocalFacts,
    observer: Option<&dyn ClaimObserver>,
    finality_depth: u64,
    bond: Option<TransactionOutpoint>,
) -> StatusReport {
    let (Some(observer), Some(claim_id), Some(bond)) = (observer, local.claim_id, bond.or(local.executor_bond)) else {
        return derive_status(local, None);
    };
    if !local.committed {
        return derive_status(local, None);
    }
    let tx_id = local.rail_txid.as_deref().and_then(parse_hash);
    match observer.observe(claim_id, tx_id) {
        Ok((observation, agreeing)) => {
            let mut tracker = ClaimTracker::new(tx_id.unwrap_or_default(), claim_id, bond, finality_depth);
            let state = tracker.observe(&observation).clone();
            derive_status(local, Some(&ChainSighting { state, provenance: Provenance::UnverifiedRemoteState { agreeing } }))
        }
        Err(e) => {
            let mut fallback = derive_status(local, None);
            fallback.note = format!("{} (the chain could not be read for this answer: {e})", fallback.note);
            fallback
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The in-flight book: what is streaming right now, and what the gateway cancelled.
// ---------------------------------------------------------------------------------------------

/// Most cancelled ids remembered (so a client can ask "what happened to my request?").
const CANCELLED_MEMORY: usize = 256;

#[derive(Default)]
struct Book {
    streaming: HashSet<String>,
    cancelled: VecDeque<String>,
}

/// Requests in flight and recently cancelled. A restart forgets both, which is correct: nothing in flight survives it, and a
/// cancelled request left no file.
#[derive(Default)]
pub struct RequestBook {
    inner: Mutex<Book>,
}

/// A request's place in the book while it streams; leaving it (finish, error, cancel) removes it.
pub struct StreamTicket<'a> {
    book: &'a RequestBook,
    id: String,
}

impl RequestBook {
    pub fn begin(&self, id: &str) -> StreamTicket<'_> {
        self.inner.lock().expect("the book lock is never poisoned").streaming.insert(id.to_string());
        StreamTicket { book: self, id: id.to_string() }
    }

    pub fn is_streaming(&self, id: &str) -> bool {
        self.inner.lock().expect("the book lock is never poisoned").streaming.contains(id)
    }

    pub fn was_cancelled(&self, id: &str) -> bool {
        self.inner.lock().expect("the book lock is never poisoned").cancelled.iter().any(|c| c == id)
    }

    pub fn streaming_count(&self) -> usize {
        self.inner.lock().expect("the book lock is never poisoned").streaming.len()
    }

    pub fn cancelled_count(&self) -> usize {
        self.inner.lock().expect("the book lock is never poisoned").cancelled.len()
    }
}

impl StreamTicket<'_> {
    /// The client left: remember it (bounded) so the id answers `cancelled`.
    pub fn cancelled(self) {
        let mut book = self.book.inner.lock().expect("the book lock is never poisoned");
        if book.cancelled.len() >= CANCELLED_MEMORY {
            book.cancelled.pop_front();
        }
        book.cancelled.push_back(self.id.clone());
        // `Drop` removes it from `streaming`.
    }
}

impl Drop for StreamTicket<'_> {
    fn drop(&mut self) {
        self.book.inner.lock().unwrap_or_else(|e| e.into_inner()).streaming.remove(&self.id);
    }
}

/// The status of an id the book knows without the outbox: `streaming` or `cancelled`.
pub fn status_from_book(book: &RequestBook, id: &str) -> Option<StatusReport> {
    if book.is_streaming(id) {
        return Some(report(
            RequestStatus::Streaming,
            None,
            None,
            "tokens shown so far are provisional: no commitment exists until the run ends, and nothing here is a result",
        ));
    }
    book.was_cancelled(id)
        .then(|| report(RequestStatus::Cancelled, None, None, "the client left before the answer was committed; nothing was written and no claim exists"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_remote::track::{ClaimObs, ClaimPhaseObs};

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond() -> TransactionOutpoint {
        TransactionOutpoint::new(h(9), 0)
    }
    fn committed() -> LocalFacts {
        LocalFacts { committed: true, claim_id: Some(h(1)), job_id: Some(h(2)), output_root: Some(h(3)), executor_bond: Some(bond()), ..Default::default() }
    }
    struct Fixed(ChainObservation, usize);
    impl ClaimObserver for Fixed {
        fn observe(&self, _: Hash64, _: Option<Hash64>) -> Result<(ChainObservation, usize), String> {
            Ok((self.0.clone(), self.1))
        }
    }
    struct Broken;
    impl ClaimObserver for Broken {
        fn observe(&self, _: Hash64, _: Option<Hash64>) -> Result<(ChainObservation, usize), String> {
            Err("connection refused".into())
        }
    }
    fn obs(daa: u64, mempool: bool, claim: Option<(u8, ClaimPhaseObs)>) -> ChainObservation {
        ChainObservation {
            sink: h(7),
            virtual_daa: daa,
            tx_in_mempool: mempool,
            claim: claim.map(|(b, phase)| ClaimObs { executor_bond: TransactionOutpoint::new(h(b), 0), accepted_block: h(5), accepted_daa: 100, phase }),
        }
    }

    #[test]
    fn only_the_chains_settled_final_is_final() {
        let all = [
            RequestStatus::Streaming,
            RequestStatus::Answered,
            RequestStatus::Committed,
            RequestStatus::Submitted,
            RequestStatus::Voided,
            RequestStatus::Cancelled,
            RequestStatus::Misattributed,
        ];
        assert!(all.iter().all(|s| !s.is_final()), "no status but `final` is final");
        assert!(RequestStatus::Final.is_final());
        // A committed, submitted or rail-confirmed job with NO chain fact is never final, whatever the files say.
        let mut local = committed();
        assert_eq!(derive_status(&local, None).status, RequestStatus::Committed);
        local.rail_txid = Some("ab".repeat(64));
        local.rail_relayed = true;
        let r = derive_status(&local, None);
        assert_eq!(r.status, RequestStatus::Submitted);
        assert!(!r.is_final() && r.provenance.is_none(), "no chain fact was read, so no label is claimed");
    }

    #[test]
    fn final_arrives_only_from_a_settled_chain_row_and_carries_the_unverified_label() {
        let local = committed();
        // Final but shallower than the finality depth: still reversible, so still `submitted`.
        let shallow = Fixed(obs(130, false, Some((9, ClaimPhaseObs::Final { final_daa: 120 }))), 2);
        let r = status_with_chain(&local, Some(&shallow), 60, None);
        assert_eq!((r.status, r.chain_state.as_deref()), (RequestStatus::Submitted, Some("FinalPending")), "{r:?}");
        // Deep enough: final — and labelled, because two nodes agreeing is not a proof.
        let deep = Fixed(obs(300, false, Some((9, ClaimPhaseObs::Final { final_daa: 120 }))), 2);
        let r = status_with_chain(&local, Some(&deep), 60, None);
        assert_eq!(r.status, RequestStatus::Final);
        assert!(r.provenance.as_deref().unwrap().starts_with("UNVERIFIED_REMOTE_STATE"), "{r:?}");
        assert!(r.to_json(&local)["final"].as_bool().unwrap());
        // Voided, settled.
        let void = Fixed(obs(300, false, Some((9, ClaimPhaseObs::Voided { voided_daa: 120 }))), 1);
        assert_eq!(status_with_chain(&local, Some(&void), 60, None).status, RequestStatus::Voided);
        // Another executor's row is not ours.
        let other = Fixed(obs(300, false, Some((4, ClaimPhaseObs::Final { final_daa: 120 }))), 1);
        assert_eq!(status_with_chain(&local, Some(&other), 60, None).status, RequestStatus::Misattributed);
    }

    #[test]
    fn a_claim_the_chain_has_not_seen_stays_committed_and_one_in_a_pool_is_submitted() {
        let local = committed();
        let none = Fixed(obs(100, false, None), 1);
        assert_eq!(status_with_chain(&local, Some(&none), 60, None).status, RequestStatus::Committed);
        let pooled = Fixed(obs(100, true, None), 1);
        let r = status_with_chain(&local, Some(&pooled), 60, None);
        assert_eq!((r.status, r.chain_state.as_deref()), (RequestStatus::Submitted, Some("Relayed")));
        for phase in [ClaimPhaseObs::Provisional, ClaimPhaseObs::ReceiptLicensed { licensed_daa: 110 }, ClaimPhaseObs::Challengeable { ends_daa: 400 }] {
            let r = status_with_chain(&local, Some(&Fixed(obs(120, false, Some((9, phase))), 1)), 60, None);
            assert_eq!(r.status, RequestStatus::Submitted, "{r:?}");
            assert!(!r.is_final());
        }
    }

    #[test]
    fn an_unreadable_chain_is_said_and_never_guessed() {
        let local = committed();
        let r = status_with_chain(&local, Some(&Broken), 60, None);
        assert_eq!(r.status, RequestStatus::Committed);
        assert!(r.note.contains("could not be read") && r.provenance.is_none(), "{r:?}");
        // A job that was not committed has no claim to ask about, even with an observer.
        let answered = LocalFacts { committed: false, not_committed_because: Some("budget".into()), ..Default::default() };
        assert_eq!(status_with_chain(&answered, Some(&Broken), 60, None).status, RequestStatus::Answered);
    }

    #[test]
    fn a_completion_id_names_one_stem_and_nothing_else_does() {
        let id = format!("palwcmpl-{}", "ab".repeat(12));
        assert_eq!(stem_of_completion_id(&id).as_deref(), Some("fp-job-abababababababab"));
        assert!(stem_of_completion_id("palwcmpl-../../etc/passwd").is_none());
        assert!(stem_of_completion_id(&format!("palwcmpl-{}", "g".repeat(24))).is_none());
        assert!(stem_of_completion_id("palwcmpl-ab").is_none());
        assert!(stem_of_completion_id(&"ab".repeat(12)).is_none());
    }

    #[test]
    fn the_book_knows_streaming_and_cancelled_and_forgets_nothing_else() {
        let book = RequestBook::default();
        assert!(status_from_book(&book, "palwcmpl-x").is_none());
        let ticket = book.begin("palwcmpl-x");
        let r = status_from_book(&book, "palwcmpl-x").expect("streaming");
        assert_eq!(r.status, RequestStatus::Streaming);
        assert!(!r.is_final() && r.note.contains("provisional"));
        assert_eq!(book.streaming_count(), 1);
        ticket.cancelled();
        assert_eq!(book.streaming_count(), 0, "a ticket leaves the in-flight set when it ends");
        assert_eq!(status_from_book(&book, "palwcmpl-x").unwrap().status, RequestStatus::Cancelled);
        // A finished (non-cancelled) request is simply not in the book.
        drop(book.begin("palwcmpl-y"));
        assert!(status_from_book(&book, "palwcmpl-y").is_none());
        // The cancelled memory is bounded.
        for i in 0..(CANCELLED_MEMORY + 10) {
            book.begin(&format!("c{i}")).cancelled();
        }
        assert_eq!(book.cancelled_count(), CANCELLED_MEMORY);
        assert!(!book.was_cancelled("palwcmpl-x"), "the oldest was forgotten");
    }

    #[test]
    fn local_facts_are_read_from_the_outbox_by_direct_probe() {
        let d = std::env::temp_dir().join(format!("misaka-gw-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let hex = |n: u8| faster_hex::hex_string(h(n).as_byte_slice());
        std::fs::write(
            d.join("fp-job-aaaaaaaaaaaaaaaa.json"),
            serde_json::json!({ "committed": true, "fp_claim_id": hex(1), "fp_job_id": hex(2), "output_root": hex(3) }).to_string(),
        )
        .unwrap();
        let facts = read_local_facts(&d, "fp-job-aaaaaaaaaaaaaaaa").expect("a summary");
        assert!(facts.committed && facts.claim_id == Some(h(1)) && !facts.expired && facts.rail_txid.is_none());
        assert_eq!(derive_status(&facts, None).status, RequestStatus::Committed);
        // The rail's record makes it `submitted`; the retired marker is reported.
        std::fs::write(d.join("fp-job-aaaaaaaaaaaaaaaa.rail.json"), serde_json::json!({ "submitted": "tx1", "relayed": null }).to_string()).unwrap();
        std::fs::write(d.join(format!("fp-job-aaaaaaaaaaaaaaaa.commitment-unsigned.borsh{}", misaka_palw_fp_submit::EXPIRED_SUFFIX)), b"").unwrap();
        let facts = read_local_facts(&d, "fp-job-aaaaaaaaaaaaaaaa").unwrap();
        assert_eq!((facts.rail_txid.as_deref(), facts.expired), (Some("tx1"), true));
        assert_eq!(derive_status(&facts, None).status, RequestStatus::Submitted);
        assert!(read_local_facts(&d, "fp-job-bbbbbbbbbbbbbbbb").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
