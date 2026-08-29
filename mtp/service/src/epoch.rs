//! The epoch pipeline (ADR-0038 D5, G1/I-MTP-1, G3/I-MTP-3).
//!
//! One weekly run, wired from pieces that are each pure and independently tested:
//!
//! 1. **Fresh single-epoch fact store (G3/I-MTP-3).** [`crate::store::PersistentStore::window`]
//!    projects only `[monday, monday+7d)` activity, so a prior epoch's facts can
//!    never leak in and re-running the cron is byte-idempotent.
//! 2. **Single-attribution resolution (G1/I-MTP-1).** [`resolve_attribution`]
//!    keeps only facts carrying a currently-registered canonical ledger id; every
//!    other fact is **dropped, not bucketed** (fail-closed). Unregistered
//!    contributions score zero. This is what closes identity-namespace splitting:
//!    a fact reaches the scorer only under the one `gh:<handle>` its author
//!    registered, so `d_n` and the 5 % cap bind per registered identity.
//! 3. **Score + sign** via the deterministic core, then **publish** the signed
//!    ledger into the append-only [`crate::publish::LedgerArchive`].
//!
//! The attribution *rewrite* (raw author key → canonical id) happens at ingestion,
//! where each collector knows its source key type (token / address / handle);
//! [`resolve_attribution`] is the epoch-time enforcement that no un-canonical or
//! un-registered id survives into a signed ledger regardless of ingestion bugs.

use kaspa_pq_validator_core::ValidatorKey;
use misaka_mtp::{ContributionEntry, EpochInput, EpochLedger, Rules, score_epoch};
use misaka_mtp_collectors::{EpochWindow, FactStore, build_epoch_input};
use std::collections::BTreeSet;

use crate::publish::{ArchiveError, LedgerArchive};
use crate::registry::Attributor;
use crate::store::PersistentStore;

#[derive(thiserror::Error, Debug)]
pub enum EpochError {
    #[error("epoch window bound {0} is not RFC-3339: {1}")]
    BadRange(String, String),
    #[error("epoch window end precedes start ({start} .. {end})")]
    EmptyRange { start: String, end: String },
    #[error("archive error: {0}")]
    Archive(#[from] ArchiveError),
}

/// Parse the `[start, end)` RFC-3339 window bounds into Unix-millisecond filters.
fn range_to_ms(range: &[String; 2]) -> Result<(u64, u64), EpochError> {
    let parse = |s: &String| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.timestamp_millis().max(0) as u64)
            .map_err(|e| EpochError::BadRange(s.clone(), e.to_string()))
    };
    let start = parse(&range[0])?;
    let end = parse(&range[1])?;
    if end <= start {
        return Err(EpochError::EmptyRange { start: range[0].clone(), end: range[1].clone() });
    }
    Ok((start, end))
}

/// Enforce single-attribution (I-MTP-1 / G1): return a new [`FactStore`] holding
/// only facts whose author id is a currently-registered canonical ledger id.
/// Facts failing the check are **dropped, not bucketed** — fail-closed, so an
/// unregistered (or de-registered) author scores zero. A dropped node also drops
/// its uptime samples, so no orphaned evidence lingers. Pure and total; running it
/// twice on the same inputs yields byte-identical output.
pub fn resolve_attribution(raw: &FactStore, attr: &Attributor) -> FactStore {
    let keep = |id: &str| attr.is_registered_id(id);

    let nodes: Vec<_> = raw.nodes.iter().filter(|n| keep(&n.owner_id)).cloned().collect();
    let kept_keys: BTreeSet<&str> = nodes.iter().map(|n| n.node_key.as_str()).collect();
    let uptime_samples = raw.uptime_samples.iter().filter(|s| kept_keys.contains(s.node_key.as_str())).cloned().collect();

    FactStore {
        // identities are evidence-only (build_epoch_input ignores them); carry them.
        identities: raw.identities.clone(),
        nodes,
        uptime_samples,
        attestations: raw.attestations.iter().filter(|a| keep(&a.validator_id)).cloned().collect(),
        gh_events: raw.gh_events.iter().filter(|e| keep(&e.reporter_id)).cloned().collect(),
        submissions: raw.submissions.iter().filter(|s| keep(&s.author_id)).cloned().collect(),
        chain_fixed: raw.chain_fixed.iter().filter(|c| keep(&c.author_id)).cloned().collect(),
        // C5: a replica slot scores only if its provider-bond owner resolved to a REGISTERED id.
        // Unregistered work is dropped rather than parked, so points can never be re-attributed to
        // an account claimed after the fact (ADR-0040 §16″ — registration is a precondition of the
        // job, not a later claim on it).
        llm_replica_work: raw.llm_replica_work.iter().filter(|w| keep(&w.owner_id)).cloned().collect(),
    }
}

/// Build **and sign** the epoch ledger and return it together with the exact
/// [`EpochInput`] it scored (G3 fresh window → G1 resolution → deterministic score
/// → operator signature). The content — scores, `inputs_hash`, `rules_hash`,
/// `digest()` — is a deterministic function of `(store window, registrations,
/// rules)`; only the ML-DSA signature bytes may vary (see the idempotency test).
/// The returned input is what a verifier recomputes against (D3).
pub fn build_epoch(
    store: &PersistentStore,
    attr: &Attributor,
    rules: &Rules,
    operator_key: &ValidatorKey,
    window: &EpochWindow,
    // Verification-required contributions (C2 bug, C3/C4 verify/infra), already filtered to
    // this `(epoch, network)` and hand-curated by the operator (`misaka mtp award`). Empty for
    // the auto-only path. See [`misaka_mtp_collectors::load_manual_awards`].
    manual: &[ContributionEntry],
) -> Result<(EpochLedger, EpochInput), EpochError> {
    let (start_ms, end_ms) = range_to_ms(&window.range)?;
    let raw = store.window(start_ms, end_ms);
    let resolved = resolve_attribution(&raw, attr);
    let input = build_epoch_input(window, &resolved, manual);
    let mut ledger = score_epoch(&input, rules);
    ledger.sign(operator_key);
    Ok((ledger, input))
}

/// Build and sign the epoch ledger (convenience wrapper over [`build_epoch`] when
/// the caller does not need the facts).
pub fn build_epoch_ledger(
    store: &PersistentStore,
    attr: &Attributor,
    rules: &Rules,
    operator_key: &ValidatorKey,
    window: &EpochWindow,
    manual: &[ContributionEntry],
) -> Result<EpochLedger, EpochError> {
    Ok(build_epoch(store, attr, rules, operator_key, window, manual)?.0)
}

/// Run one epoch end-to-end: [`build_epoch`] then publish the signed ledger **and
/// its facts sidecar** into `archive` as a new issue (D6). Publishing the facts is
/// what makes the D3 self-verification recompute fully trustless. The publish is
/// append-only and finality-horizon-guarded by the archive.
pub fn run_epoch(
    store: &PersistentStore,
    attr: &Attributor,
    rules: &Rules,
    operator_key: &ValidatorKey,
    window: &EpochWindow,
    archive: &mut LedgerArchive,
    manual: &[ContributionEntry],
) -> Result<EpochLedger, EpochError> {
    let (ledger, input) = build_epoch(store, attr, rules, operator_key, window, manual)?;
    let input_json = serde_json::to_string(&input).expect("EpochInput JSON is infallible");
    archive.publish_with_input(&ledger, &input_json, "", "")?;
    Ok(ledger)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_hashes::blake2b_512_address_payload;
    use misaka_mtp::{Category, Contribution, Severity, Stage};
    use misaka_mtp_collectors::{AttestationRow, GhEvent, NodeRecord, Submission, UptimeSample};
    use std::path::PathBuf;

    use crate::registry::{NonceStore, RegistrationRecord};

    fn tempdir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let uniq = format!("mtp-epoch-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
        let p = std::env::temp_dir().join(uniq);
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // Register `handle` and return (record, address). Uses a real ML-DSA key so the
    // full binding path runs; the claim-token / address / handle all resolve to the
    // one canonical id.
    fn register(attr: &mut Attributor, handle: &str, seed: u8) -> (RegistrationRecord, String) {
        let key = ValidatorKey::from_seed([seed; 32]);
        let pk = key.public_key().to_vec();
        let payload = blake2b_512_address_payload(&pk);
        let addr = Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &payload.as_bytes()).to_string();
        let mut ns = NonceStore::new();
        let nonce = [seed; 32];
        let nonce_hex = faster_hex::hex_string(&nonce);
        let challenge = ns.issue("testnet-10", handle, &addr, nonce, 1000);
        let sig = key.sign_with_context(&challenge, misaka_mtp::MTP_REGISTER_CONTEXT);
        let rec = attr
            .register(
                &mut ns,
                "testnet-10",
                handle,
                &addr,
                &pk,
                &nonce_hex,
                &sig,
                1000,
                Prefix::Testnet,
                misaka_mtp::LedgerAttribution::Github,
            )
            .unwrap();
        (rec, addr)
    }

    fn window() -> EpochWindow {
        EpochWindow {
            epoch: 12,
            range: ["2026-09-21T00:00:00Z".into(), "2026-09-28T00:00:00Z".into()],
            network: "testnet-10".into(),
            stage: Stage::A,
        }
    }

    // Unix ms for a time inside the window() range.
    fn in_window_ms() -> u64 {
        chrono::DateTime::parse_from_rfc3339("2026-09-22T00:00:00Z").unwrap().timestamp_millis() as u64
    }

    /// §5 test 1 — attribution round-trip: facts from all four source types land
    /// under the ONE canonical id; the same facts with no registration score zero.
    #[test]
    fn on_chain_facts_credit_the_address_and_off_chain_facts_the_registration() {
        let mut attr = Attributor::new();
        let (rec, addr) = register(&mut attr, "alice", 0x11);
        let id = rec.ledger_id(); // "gh:alice"
        let ts = in_window_ms();

        // Simulate ingestion: each collector resolves its source key to the canonical
        // id before storing (node via token, chain via address, gh via handle).
        let node_owner = attr.resolve_token(&rec.claim_token).unwrap().to_string();
        // 2026-08-02: chain facts resolve to the ADDRESS id, which is deliberately NOT the
        // registration's `gh:` id — on-chain work is credited to the address that did it.
        let val_id = attr.resolve_address_id(&addr).unwrap();
        let bug_id = attr.resolve_github("alice").unwrap().to_string();
        let sub_id = attr.resolve_github("alice").unwrap().to_string();
        assert!([&node_owner, &bug_id, &sub_id].iter().all(|x| **x == id));
        assert_eq!(val_id, format!("addr:{addr}"), "chain facts are credited to the address");

        let dir = tempdir("collapse");
        let mut store = PersistentStore::new(&dir);
        store
            .upsert_node(NodeRecord {
                node_key: "n1".into(),
                owner_id: node_owner,
                ip_v4_24: Some([10, 0, 1]),
                asn: Some(100),
                geo_diverse: false,
                fast_follow: false,
                first_seen_ms: 1,
            })
            .unwrap();
        store
            .append_sample(UptimeSample {
                node_key: "n1".into(),
                at_ms: ts,
                in_sync: true,
                vantage: "DE".into(),
                evidence: "s1".into(),
            })
            .unwrap();
        store
            .append_attestation(
                ts,
                AttestationRow { validator_id: val_id, att_epoch: 5, attested: true, slashed: false, evidence: "att1".into() },
            )
            .unwrap();
        store
            .append_gh_event(
                ts,
                GhEvent {
                    reporter_id: bug_id,
                    severity: Severity::S1,
                    first_report: true,
                    fix_pr_accepted: false,
                    evidence: "gh#1".into(),
                },
            )
            .unwrap();
        store
            .append_submission(
                ts,
                Submission { author_id: sub_id, category: Category::Verify, base_points: 30, evidence: "form#1".into() },
            )
            .unwrap();

        let key = ValidatorKey::from_seed([0xAB; 32]);
        // C2 bug + C3 verify are verification-required, so the auto pipeline no longer scores the
        // gh_event / submission facts stored above. The operator adds them by hand as manual
        // awards carrying the canonical ledger id ("gh:alice"). The node + validator facts are
        // still auto-collected and resolved through attribution.
        let manual = vec![
            ContributionEntry {
                id: id.clone(),
                contribution: Contribution::Bug { severity: Severity::S1, first_report: true, fix_pr_accepted: false },
                evidence: vec!["gh#1".into()],
            },
            ContributionEntry {
                id: id.clone(),
                contribution: Contribution::Fixed { category: Category::Verify, base_points: 30 },
                evidence: vec!["form#1".into()],
            },
        ];
        let ledger = build_epoch_ledger(&store, &attr, &Rules::default(), &key, &window(), &manual).unwrap();

        // 2026-08-02 — the collapse is now PARTIAL, and that is the intended consequence of
        // crediting on-chain work to the address that did it. Off-chain facts (node claim token,
        // GitHub handle, campaign forms) still collapse onto the registration's one id; ON-CHAIN
        // facts land on `addr:…` instead, because the chain names an address and nothing else.
        //
        // Stated plainly because it is a real trade the address policy makes: one human who both
        // runs a validator and files bugs now appears as TWO ledger ids. Linking them back together
        // is an ALLOCATION-time decision for the operator, not something this function may smuggle
        // in — doing it here would require the registration lookup whose absence is the point.
        assert_eq!(ledger.scores.len(), 2, "on-chain facts bucket by address, off-chain facts by registration");
        let addr_row = ledger.scores.iter().find(|r| r.id.starts_with("addr:")).expect("an address row");
        assert_eq!(addr_row.id, format!("addr:{addr}"));
        assert!(addr_row.c1 > 0, "the validator's on-chain work is credited to its address");

        let row = ledger.scores.iter().find(|r| r.id == "gh:alice").expect("the registration row");
        assert!(row.c1 > 0, "node uptime is claim-token attributed and stays on the registration id");
        assert_eq!(row.c2, 2_000_000, "S1 first bug (manual award)");
        assert_eq!(row.c3, 30_000, "verify submission (manual award)");

        // Now drop the registration entirely. This is the assertion the 2026-08-02 change INVERTS,
        // so it is worth being explicit about what it used to say: "unregistered auto facts score
        // zero (fail-closed)". That fail-closed rule protected nothing — the facts are chain-derived
        // and already finality-checked — while quietly deleting the points of anyone who earned
        // on-chain and never visited the registration service.
        //
        // Now the chain facts survive, credited to the address that produced them. The token- and
        // handle-attributed facts still vanish, because without a registration there is genuinely no
        // id to attribute them to: a claim token names a registration and nothing else.
        let empty_attr = Attributor::new();
        let empty = build_epoch_ledger(&store, &empty_attr, &Rules::default(), &key, &window(), &[]).unwrap();
        let ids: Vec<&str> = empty.scores.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec![format!("addr:{addr}").as_str()], "on-chain facts are credited without any registration");
        assert!(empty.scores[0].c1 > 0);
    }

    /// §5 test 5 — idempotent epoch: build twice → identical content; a prior
    /// epoch's facts never appear in this one.
    #[test]
    fn epoch_build_is_idempotent_and_window_scoped() {
        let mut attr = Attributor::new();
        let (rec, _addr) = register(&mut attr, "carol", 0x22);
        let id = rec.ledger_id();
        let dir = tempdir("idem");
        let mut store = PersistentStore::new(&dir);
        store
            .upsert_node(NodeRecord {
                node_key: "n1".into(),
                owner_id: id.clone(),
                ip_v4_24: Some([10, 0, 2]),
                asn: Some(7),
                geo_diverse: false,
                fast_follow: false,
                first_seen_ms: 1,
            })
            .unwrap();
        // one sample INSIDE this epoch's window, one WAY before it (prior epoch).
        store
            .append_sample(UptimeSample {
                node_key: "n1".into(),
                at_ms: in_window_ms(),
                in_sync: true,
                vantage: "DE".into(),
                evidence: "in".into(),
            })
            .unwrap();
        store
            .append_sample(UptimeSample {
                node_key: "n1".into(),
                at_ms: 1,
                in_sync: true,
                vantage: "DE".into(),
                evidence: "prior".into(),
            })
            .unwrap();

        let key = ValidatorKey::from_seed([0xCD; 32]);
        let l1 = build_epoch_ledger(&store, &attr, &Rules::default(), &key, &window(), &[]).unwrap();
        let l2 = build_epoch_ledger(&store, &attr, &Rules::default(), &key, &window(), &[]).unwrap();
        // content is byte-idempotent (signature bytes aside).
        assert_eq!(l1.digest(), l2.digest());
        assert_eq!(l1.scores, l2.scores);
        assert_eq!(l1.inputs_hash, l2.inputs_hash);
        // the prior-epoch sample is out of window → only 1 sample counted (100% uptime).
        let row = l1.scores.iter().find(|s| s.id == id).unwrap();
        assert_eq!(row.evidence, vec!["in".to_string()], "prior-epoch evidence excluded");
    }

    /// run_epoch publishes a verifiable signed ledger into the archive.
    #[test]
    fn run_epoch_publishes_signed_ledger() {
        let mut attr = Attributor::new();
        let (rec, _addr) = register(&mut attr, "dave", 0x33);
        let dir = tempdir("run");
        let mut store = PersistentStore::new(dir.join("facts"));
        store
            .upsert_node(NodeRecord {
                node_key: "n1".into(),
                owner_id: rec.ledger_id(),
                ip_v4_24: Some([10, 0, 3]),
                asn: Some(7),
                geo_diverse: false,
                fast_follow: false,
                first_seen_ms: 1,
            })
            .unwrap();
        store
            .append_sample(UptimeSample {
                node_key: "n1".into(),
                at_ms: in_window_ms(),
                in_sync: true,
                vantage: "DE".into(),
                evidence: "s".into(),
            })
            .unwrap();

        let key = ValidatorKey::from_seed([0xEF; 32]);
        let mut archive = LedgerArchive::open(dir.join("points")).unwrap();
        let ledger = run_epoch(&store, &attr, &Rules::default(), &key, &window(), &mut archive, &[]).unwrap();
        assert!(ledger.verify(key.public_key()));
        // it's in the archive at issue 0 and reads back verbatim.
        assert_eq!(archive.latest(window().epoch).unwrap().issue, 0);
        assert_eq!(archive.read_ledger(window().epoch, 0).unwrap(), ledger);
    }

    /// §5 test 10 — self-verification: the published facts recompute to the exact
    /// published ledger (signature + rules-hash + byte-compare); a one-mpt tamper
    /// of the ledger breaks the signature.
    #[test]
    fn published_facts_recompute_to_the_signed_ledger() {
        let mut attr = Attributor::new();
        let (rec, _addr) = register(&mut attr, "erin", 0x44);
        let dir = tempdir("selfverify");
        let mut store = PersistentStore::new(dir.join("facts"));
        store
            .upsert_node(NodeRecord {
                node_key: "n1".into(),
                owner_id: rec.ledger_id(),
                ip_v4_24: Some([10, 0, 4]),
                asn: Some(7),
                geo_diverse: false,
                fast_follow: false,
                first_seen_ms: 1,
            })
            .unwrap();
        store
            .append_sample(UptimeSample {
                node_key: "n1".into(),
                at_ms: in_window_ms(),
                in_sync: true,
                vantage: "DE".into(),
                evidence: "s".into(),
            })
            .unwrap();

        let key = ValidatorKey::from_seed([0x99; 32]);
        let mut archive = LedgerArchive::open(dir.join("points")).unwrap();
        let published = run_epoch(&store, &attr, &Rules::default(), &key, &window(), &mut archive, &[]).unwrap();

        // (1) signature verifies against the operator pubkey.
        assert!(published.verify(key.public_key()));
        // (2) rules-hash matches the published rules doc.
        assert_eq!(published.rules_hash, faster_hex::hex_string(&Rules::default().rules_hash().as_bytes()));
        // (3) recompute from the PUBLISHED facts sidecar → byte-identical content.
        let facts_json = archive.read_input_json(window().epoch, 0).unwrap().expect("facts were published");
        let input: misaka_mtp::EpochInput = serde_json::from_str(&facts_json).unwrap();
        let mut recomputed = misaka_mtp::score_epoch(&input, &Rules::default());
        recomputed.sig_mldsa87 = None;
        let mut published_unsigned = published.clone();
        published_unsigned.sig_mldsa87 = None;
        assert_eq!(recomputed, published_unsigned, "published facts recompute to the published ledger");

        // a one-milli-point tamper of the ledger breaks the signature.
        let mut tampered = published.clone();
        tampered.scores[0].c1 += 1;
        assert!(!tampered.verify(key.public_key()), "any edit invalidates the signature");
    }

    #[test]
    fn bad_or_empty_range_is_rejected() {
        let store = PersistentStore::new(tempdir("bad"));
        let attr = Attributor::new();
        let key = ValidatorKey::from_seed([1; 32]);
        let mut w = window();
        w.range = ["not-a-date".into(), "2026-09-28T00:00:00Z".into()];
        assert!(matches!(build_epoch_ledger(&store, &attr, &Rules::default(), &key, &w, &[]), Err(EpochError::BadRange(..))));
        w.range = ["2026-09-28T00:00:00Z".into(), "2026-09-21T00:00:00Z".into()];
        assert!(matches!(build_epoch_ledger(&store, &attr, &Rules::default(), &key, &w, &[]), Err(EpochError::EmptyRange { .. })));
    }
}
