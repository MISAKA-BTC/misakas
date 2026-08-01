//! Collectors (ADR-0027 §4.1, D8): the I/O layer that fills the [`FactStore`].
//!
//! Each of the four production collectors — **p2p-crawler** (×2 vantage),
//! **chain-indexer**, **github-sync**, **campaign-forms** — implements the
//! [`Collector`] seam. In a deployment the concrete collector performs the
//! network/DB/GitHub fetch on its cron tick; the part that lives here and is
//! deterministically testable is the **normalization**: taking already-fetched
//! raw rows and writing typed §4.2 facts into the store. A [`MockCollector`]
//! feeds a fixed fact set so the whole pipeline is exercised offline — the same
//! trait-seam-plus-mock shape as `misaka-mil-provider`'s `InferenceBackend`
//! /`MockBackend`.
//!
//! The network fetch itself (dialing peers, reading the chain, calling the
//! GitHub API) is explicitly out of scope — the ADR specifies a single Rust
//! service + cron around this crate.

use crate::store::{
    AttestationRow, ChainFixed, FactStore, GhEvent, Identity, IdentityKind, LlmReplicaWork, NodeRecord, Submission,
    UptimeSample,
};
use misaka_mtp::Stage;

/// The epoch window a collection run targets (mirrors [`misaka_mtp::EpochInput`]'s
/// header). Passed to every collector so time-scoped sources agree on the range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochWindow {
    pub epoch: u64,
    /// `[start, end)` RFC-3339 UTC bounds of the weekly epoch.
    pub range: [String; 2],
    pub network: String,
    pub stage: Stage,
}

/// A collection failure (a source was unreachable / returned malformed rows).
#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error("collector {collector} source error: {reason}")]
    Source { collector: String, reason: String },
}

/// A source of raw contribution facts for one epoch window (§4.1). Real adapters
/// fetch then normalize; the [`MockCollector`] just replays a fixed fact set.
pub trait Collector {
    /// Stable collector name (for logs / the run report).
    fn name(&self) -> &str;

    /// Normalize this source's rows into `store` for `window`. Returns the number
    /// of fact rows written (across all tables it touches).
    fn collect(&self, window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError>;
}

/// Run every collector in order into a shared store, returning `(name, rows)` per
/// collector. Ordering does not affect the final ledger — the aggregator and the
/// core's `inputs_hash` are order-independent — but a stable order keeps the run
/// report reproducible.
pub fn run_all(
    collectors: &[Box<dyn Collector>],
    window: &EpochWindow,
    store: &mut FactStore,
) -> Result<Vec<(String, usize)>, CollectError> {
    let mut report = Vec::with_capacity(collectors.len());
    for c in collectors {
        let n = c.collect(window, store)?;
        report.push((c.name().to_string(), n));
    }
    Ok(report)
}

// --- p2p-crawler (×2 vantage): uptime samples + node records -----------------------------

/// The p2p-crawler collector (§4.1, §5). A crawl vantage produces per-node
/// probes; this normalizes them into `nodes` + `uptime_samples`. Two instances
/// (DE / JP) run and both write into the same store — cross-vantage agreement is
/// what earns the `m_geo` bonus at aggregation time.
pub struct P2pCrawlerCollector {
    pub vantage: String,
    /// Nodes this vantage knows about (already fetched).
    pub nodes: Vec<NodeRecord>,
    /// Probes taken this window (already fetched).
    pub samples: Vec<UptimeSample>,
}

impl Collector for P2pCrawlerCollector {
    fn name(&self) -> &str {
        "p2p-crawler"
    }

    fn collect(&self, _window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError> {
        let mut n = 0;
        for node in &self.nodes {
            store.upsert_identity(Identity { id: node.owner_id.clone(), kind: IdentityKind::Node });
            store.upsert_node(node.clone());
            n += 1;
        }
        for s in &self.samples {
            store.uptime_samples.push(s.clone());
            n += 1;
        }
        Ok(n)
    }
}

// --- chain-indexer: validator attestations + C1 fixed chain activities -------------------

/// The chain-indexer collector (§4.1). Reads the chain for validator attestations
/// (and slash events) plus IBD-benchmark / drill participation.
pub struct ChainIndexerCollector {
    pub attestations: Vec<AttestationRow>,
    pub chain_fixed: Vec<ChainFixed>,
}

impl Collector for ChainIndexerCollector {
    fn name(&self) -> &str {
        "chain-indexer"
    }

    fn collect(&self, _window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError> {
        let mut n = 0;
        for a in &self.attestations {
            store.upsert_identity(Identity { id: a.validator_id.clone(), kind: IdentityKind::Address });
            store.attestations.push(a.clone());
            n += 1;
        }
        for c in &self.chain_fixed {
            store.upsert_identity(Identity { id: c.author_id.clone(), kind: IdentityKind::Address });
            store.chain_fixed.push(c.clone());
            n += 1;
        }
        Ok(n)
    }
}

// --- github-sync: bug reports ------------------------------------------------------------

/// The github-sync collector (§4.1, §3.2). Mirrors triaged issues/PRs into
/// `gh_events` (the severity/first/fix curation is done by the triage step it
/// wraps, per D2's mandatory-private-disclosure rule).
pub struct GithubSyncCollector {
    pub events: Vec<GhEvent>,
}

impl Collector for GithubSyncCollector {
    fn name(&self) -> &str {
        "github-sync"
    }

    fn collect(&self, _window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError> {
        let mut n = 0;
        for e in &self.events {
            store.upsert_identity(Identity { id: e.reporter_id.clone(), kind: IdentityKind::Github });
            store.gh_events.push(e.clone());
            n += 1;
        }
        Ok(n)
    }
}

// --- campaign-forms: C3 verification + C4 infra submissions ------------------------------

/// The campaign-forms collector (§4.1, §3.3/§3.4). Ingests form submissions whose
/// per-event cap / tier is already resolved into `base_points`.
pub struct CampaignFormsCollector {
    pub submissions: Vec<Submission>,
}

impl Collector for CampaignFormsCollector {
    fn name(&self) -> &str {
        "campaign-forms"
    }

    fn collect(&self, _window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError> {
        let mut n = 0;
        for s in &self.submissions {
            store.upsert_identity(Identity { id: s.author_id.clone(), kind: IdentityKind::Address });
            store.submissions.push(s.clone());
            n += 1;
        }
        Ok(n)
    }
}

// --- palw-replica: C5 accepted, k=2-matched replica slots --------------------------------

/// Resolves a provider's on-chain bond-owner address to its registered MTP ledger id.
///
/// The service implements this over its `Attributor` (`resolve_address`); tests use a plain map.
/// A collector cannot depend on the service crate (the dependency runs the other way), so the
/// resolution is a seam rather than a direct call — the same shape as [`Collector`] itself.
pub trait OwnerResolver {
    /// `Some(ledger_id)` iff this address belongs to a REGISTERED participant.
    fn ledger_id_for_address(&self, address: &str) -> Option<String>;
}

impl OwnerResolver for std::collections::BTreeMap<String, String> {
    fn ledger_id_for_address(&self, address: &str) -> Option<String> {
        self.get(address).cloned()
    }
}

/// One replica slot of a PALW job, as the chain and its DA object describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicaSlot {
    /// 0 = replica A, 1 = replica B.
    pub replica_slot: u8,
    /// The Receipt-v3 execution nullifier — the global dedup key (ADR-0040 P1-9).
    pub execution_nullifier: String,
    pub worker_credential_id: String,
    /// `txid:index` of the provider bond backing this slot.
    pub provider_bond: String,
    /// The bond owner's MISAKA address, derived by the chain reader from the bond's owner
    /// public key. This is the last on-chain link before MTP registration.
    pub owner_address: String,
}

/// One accepted PALW leaf, read off the **finality-buried** selected chain.
///
/// Everything here is chain-derived. The collector re-checks the parts that decide whether points
/// are owed (match, finality, pair shape, registration) rather than trusting the fetcher, so a bug
/// or a compromise in the reader cannot mint points on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedPalwLeaf {
    pub batch_id: String,
    pub leaf_index: u32,
    /// The block that accepted this leaf's batch — the evidence link the ledger pins.
    pub accepting_block: String,
    /// DAA score of `accepting_block`, compared against the finality coordinate.
    pub accepted_daa_score: u64,
    /// Job completion time (ms) — decides which epoch the work lands in.
    pub completed_at_ms: u64,
    /// Identity of the A/B pair (`external_pair_id`).
    pub pair_id: String,
    pub job_challenge: String,
    /// The node's own eight-field `ReplicaMatchKey` / `run_replica_k2` verdict.
    pub k2_matched: bool,
    /// Committed CU (recorded as evidence; not scored — see `c5_points_per_accepted_replica`).
    pub canonical_compute_units: u64,
    /// The pair's slots. Exactly two, or the leaf is malformed and earns nothing.
    pub slots: Vec<ReplicaSlot>,
}

/// Why a leaf or slot earned nothing. Every drop is reported rather than silently skipped: a
/// collector that quietly discards work looks identical to one that found none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// The k=2 replicas did not reproduce each other. Unmatched work is unverified work.
    NotMatched { pair_id: String },
    /// Accepted above the finality coordinate. A leaf that can still be reorged out must not be
    /// scored — a point outliving its block is a ghost the ledger can never retract.
    NotFinal { pair_id: String, accepted_daa_score: u64, finality_daa_score: u64 },
    /// A PALW job is exactly two slots; anything else is evidence we do not understand.
    MalformedPair { pair_id: String, slots: usize },
    /// Receipt-v3 forbids A and B sharing a worker credential. Seeing it here means the evidence
    /// is internally inconsistent, so the whole pair is refused rather than half-credited.
    SharedCredential { pair_id: String, worker_credential_id: String },
    /// The bond owner is not a registered MTP participant. Dropped, never parked: crediting it
    /// later would let work be claimed by an account created after the fact.
    UnregisteredOwner { pair_id: String, owner_address: String },
}

/// What one normalization pass produced — the creditable rows, and every drop with its reason.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwNormalizeReport {
    pub rows: Vec<LlmReplicaWork>,
    pub rejected: Vec<Rejected>,
}

/// The palw-replica collector (C5, ADR-0040 §16″).
///
/// Normalizes accepted PALW leaves into [`LlmReplicaWork`] facts. It applies the checks that decide
/// **whether** work is creditable; the checks that decide **how much** it is worth (nullifier dedup
/// across the epoch, one credit per identity per pair) belong to the aggregator, which sees the
/// whole epoch at once and is where they are tested.
pub struct PalwReplicaCollector<R: OwnerResolver> {
    pub leaves: Vec<AcceptedPalwLeaf>,
    /// Only leaves accepted at or below this DAA score are creditable. The caller sets it to the
    /// finality-buried coordinate, NOT the tip.
    pub finality_daa_score: u64,
    pub resolver: R,
}

impl<R: OwnerResolver> PalwReplicaCollector<R> {
    /// Pure normalization — no store, no I/O. Callers that want the audit trail (which leaves were
    /// dropped and why) call this directly; [`Collector::collect`] uses it and keeps only the rows.
    pub fn normalize(&self) -> PalwNormalizeReport {
        let mut out = PalwNormalizeReport::default();
        for leaf in &self.leaves {
            if !leaf.k2_matched {
                out.rejected.push(Rejected::NotMatched { pair_id: leaf.pair_id.clone() });
                continue;
            }
            if leaf.accepted_daa_score > self.finality_daa_score {
                out.rejected.push(Rejected::NotFinal {
                    pair_id: leaf.pair_id.clone(),
                    accepted_daa_score: leaf.accepted_daa_score,
                    finality_daa_score: self.finality_daa_score,
                });
                continue;
            }
            if leaf.slots.len() != 2 {
                out.rejected.push(Rejected::MalformedPair { pair_id: leaf.pair_id.clone(), slots: leaf.slots.len() });
                continue;
            }
            if leaf.slots[0].worker_credential_id == leaf.slots[1].worker_credential_id {
                out.rejected.push(Rejected::SharedCredential {
                    pair_id: leaf.pair_id.clone(),
                    worker_credential_id: leaf.slots[0].worker_credential_id.clone(),
                });
                continue;
            }
            for slot in &leaf.slots {
                let Some(owner_id) = self.resolver.ledger_id_for_address(&slot.owner_address) else {
                    out.rejected.push(Rejected::UnregisteredOwner {
                        pair_id: leaf.pair_id.clone(),
                        owner_address: slot.owner_address.clone(),
                    });
                    continue;
                };
                out.rows.push(LlmReplicaWork {
                    completed_at_ms: leaf.completed_at_ms,
                    pair_id: leaf.pair_id.clone(),
                    job_challenge: leaf.job_challenge.clone(),
                    execution_nullifier: slot.execution_nullifier.clone(),
                    provider_bond: slot.provider_bond.clone(),
                    worker_credential_id: slot.worker_credential_id.clone(),
                    replica_slot: slot.replica_slot,
                    owner_id,
                    work_units: 1, // Phase 1: one accepted slot = one unit.
                    canonical_compute_units: leaf.canonical_compute_units,
                    evidence: format!("{}#{}:{}", leaf.accepting_block, leaf.batch_id, leaf.leaf_index),
                });
            }
        }
        out
    }
}

impl<R: OwnerResolver> Collector for PalwReplicaCollector<R> {
    fn name(&self) -> &str {
        "palw-replica"
    }

    fn collect(&self, _window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError> {
        let report = self.normalize();
        let n = report.rows.len();
        for row in report.rows {
            store.upsert_identity(Identity { id: row.owner_id.clone(), kind: IdentityKind::Address });
            store.llm_replica_work.push(row);
        }
        Ok(n)
    }
}

// --- mock: a fixed store for offline pipeline tests --------------------------------------

/// A collector that writes a caller-supplied store snapshot verbatim — the
/// offline stand-in that lets the aggregation pipeline be tested without any
/// live source (mirrors `MockBackend`).
pub struct MockCollector {
    pub facts: FactStore,
}

impl Collector for MockCollector {
    fn name(&self) -> &str {
        "mock"
    }

    fn collect(&self, _window: &EpochWindow, store: &mut FactStore) -> Result<usize, CollectError> {
        let f = &self.facts;
        store.identities.extend(f.identities.iter().cloned());
        store.nodes.extend(f.nodes.iter().cloned());
        store.uptime_samples.extend(f.uptime_samples.iter().cloned());
        store.attestations.extend(f.attestations.iter().cloned());
        store.gh_events.extend(f.gh_events.iter().cloned());
        store.submissions.extend(f.submissions.iter().cloned());
        store.chain_fixed.extend(f.chain_fixed.iter().cloned());
        store.llm_replica_work.extend(f.llm_replica_work.iter().cloned());
        Ok(f.len())
    }
}

#[cfg(test)]
mod palw_replica_tests {
    use super::*;
    use std::collections::BTreeMap;

    const FINAL_DAA: u64 = 1_000;

    fn slot(n: u8, owner: &str) -> ReplicaSlot {
        ReplicaSlot {
            replica_slot: n,
            execution_nullifier: format!("nullifier-{owner}-{n}"),
            worker_credential_id: format!("cred-{owner}-{n}"),
            provider_bond: format!("bond-{owner}:0"),
            owner_address: format!("misakatest:{owner}"),
        }
    }

    fn leaf(pair: &str, a: &str, b: &str) -> AcceptedPalwLeaf {
        AcceptedPalwLeaf {
            batch_id: format!("batch-{pair}"),
            leaf_index: 0,
            accepting_block: format!("block-{pair}"),
            accepted_daa_score: 900, // buried
            completed_at_ms: 1_700_000_000_000,
            pair_id: pair.into(),
            job_challenge: "challenge".into(),
            k2_matched: true,
            canonical_compute_units: 781_556,
            slots: vec![slot(0, a), slot(1, b)],
        }
    }

    fn resolver(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(addr, id)| (format!("misakatest:{addr}"), (*id).to_string())).collect()
    }

    fn run(leaves: Vec<AcceptedPalwLeaf>, reg: &[(&str, &str)]) -> PalwNormalizeReport {
        PalwReplicaCollector { leaves, finality_daa_score: FINAL_DAA, resolver: resolver(reg) }.normalize()
    }

    /// The happy path, and the shape the ledger consumes: one row per slot, each attributed to its
    /// own registered id and citing the block that accepted it.
    #[test]
    fn an_accepted_matched_pair_yields_one_row_per_slot() {
        let r = run(vec![leaf("p1", "alice", "bob")], &[("alice", "gh:alice"), ("bob", "gh:bob")]);
        assert!(r.rejected.is_empty(), "nothing to reject: {:?}", r.rejected);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0].owner_id, "gh:alice");
        assert_eq!(r.rows[1].owner_id, "gh:bob");
        assert!(r.rows.iter().all(|w| w.work_units == 1));
        assert!(r.rows.iter().all(|w| w.evidence.starts_with("block-p1#")), "every row cites its accepting block");
        assert_eq!(r.rows[0].canonical_compute_units, 781_556, "CU is carried as evidence");
    }

    /// Unmatched work is unverified work — it pays nothing, and the drop is reported.
    #[test]
    fn an_unmatched_pair_earns_nothing() {
        let mut l = leaf("p1", "alice", "bob");
        l.k2_matched = false;
        let r = run(vec![l], &[("alice", "gh:alice"), ("bob", "gh:bob")]);
        assert!(r.rows.is_empty());
        assert_eq!(r.rejected, vec![Rejected::NotMatched { pair_id: "p1".into() }]);
    }

    /// A leaf that could still be reorged out must not be scored: the ledger cannot retract a point.
    #[test]
    fn work_above_the_finality_coordinate_is_not_scored_yet() {
        let mut l = leaf("p1", "alice", "bob");
        l.accepted_daa_score = FINAL_DAA + 1;
        let r = run(vec![l], &[("alice", "gh:alice"), ("bob", "gh:bob")]);
        assert!(r.rows.is_empty(), "not buried ⇒ not creditable");
        assert!(matches!(r.rejected.as_slice(), [Rejected::NotFinal { accepted_daa_score, .. }] if *accepted_daa_score == FINAL_DAA + 1));
    }

    /// Receipt-v3 forbids A and B sharing a credential; if the evidence says otherwise it is
    /// inconsistent, and the whole pair is refused rather than half-credited.
    #[test]
    fn a_pair_sharing_one_worker_credential_is_refused_entirely() {
        let mut l = leaf("p1", "alice", "bob");
        l.slots[1].worker_credential_id = l.slots[0].worker_credential_id.clone();
        let r = run(vec![l], &[("alice", "gh:alice"), ("bob", "gh:bob")]);
        assert!(r.rows.is_empty());
        assert!(matches!(r.rejected.as_slice(), [Rejected::SharedCredential { .. }]));
    }

    /// A job is exactly two slots; anything else is evidence we do not understand.
    #[test]
    fn a_malformed_pair_is_refused() {
        let mut l = leaf("p1", "alice", "bob");
        l.slots.pop();
        let r = run(vec![l], &[("alice", "gh:alice"), ("bob", "gh:bob")]);
        assert!(r.rows.is_empty());
        assert_eq!(r.rejected, vec![Rejected::MalformedPair { pair_id: "p1".into(), slots: 1 }]);
    }

    /// Unregistered work is dropped, not parked — and the registered half is still paid.
    #[test]
    fn an_unregistered_owner_is_dropped_and_its_counterparty_is_not() {
        let r = run(vec![leaf("p1", "alice", "stranger")], &[("alice", "gh:alice")]);
        assert_eq!(r.rows.len(), 1, "alice is paid");
        assert_eq!(r.rows[0].owner_id, "gh:alice");
        assert!(matches!(r.rejected.as_slice(), [Rejected::UnregisteredOwner { .. }]), "the stranger is reported, not silently skipped");
    }

    /// The collector writes through the `Collector` seam into a real store, and registers the
    /// identity so the row is attributable downstream.
    #[test]
    fn collect_writes_facts_and_identities_into_the_store() {
        let c = PalwReplicaCollector {
            leaves: vec![leaf("p1", "alice", "bob")],
            finality_daa_score: FINAL_DAA,
            resolver: resolver(&[("alice", "gh:alice"), ("bob", "gh:bob")]),
        };
        let window = EpochWindow {
            epoch: 1,
            range: ["2026-08-01T00:00:00Z".into(), "2026-08-08T00:00:00Z".into()],
            network: "testnet-20".into(),
            stage: Stage::A,
        };
        let mut store = FactStore::new();
        assert_eq!(c.collect(&window, &mut store).unwrap(), 2);
        assert_eq!(store.llm_replica_work.len(), 2);
        assert_eq!(store.identities.len(), 2, "both providers are registered identities");
        assert_eq!(c.name(), "palw-replica");
    }
}
