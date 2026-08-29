//! The raw-fact store (ADR-0027 §4.2). In a deployment this is a single SQLite
//! file with one table per row-type below; here it is the same logical schema
//! held in memory (Vec-per-table), so the normalization + §5 dedup + aggregation
//! are exercised with zero external dependencies. A SQLite-backed store can
//! implement the exact same shape without touching the aggregation logic.
//!
//! Tables (§4.2): `identities`, `nodes`, `uptime_samples`, `attestations`,
//! `gh_events`, `submissions`. Every row carries the evidence link the ledger
//! pins (a crawl sample id / on-chain attestation / GitHub issue / tx id), so the
//! §3 "every point cites its evidence" rule holds end-to-end.

use misaka_mtp::{Category, Severity};
use serde::{Deserialize, Serialize};

/// The identity an activity is attributed to (§4.2 `identities`). The `id` string
/// is the ledger key (e.g. `gh:alice`, `node:<pk>`, `addr:<bech32>`); `kind`
/// records which namespace it came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub kind: IdentityKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdentityKind {
    /// A GitHub handle (bug reports, verification tasks).
    Github,
    /// A node operator key.
    Node,
    /// An on-chain address (validator / campaign).
    Address,
}

/// A node observed by the crawlers (§4.2 `nodes`). `owner_id` is the identity the
/// node's points accrue to; `ip_v4_24` / `asn` drive the §5 co-location cap.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRecord {
    /// Stable per-node key (e.g. node pubkey / peer id).
    pub node_key: String,
    /// Identity that owns this node (points accrue here).
    pub owner_id: String,
    /// The node's /24 IPv4 prefix (first three octets), if known — the §5 cap key.
    pub ip_v4_24: Option<[u8; 3]>,
    /// The node's ASN, if known — the alternate §5 cap key.
    pub asn: Option<u32>,
    /// Whether the node is in a distinct geo/vantage from the owner's others
    /// (the §3.1 `m_geo` 1.5× multiplier).
    pub geo_diverse: bool,
    /// Whether the node fast-followed the current release (the §3.1 `m_ver` 1.2×).
    pub fast_follow: bool,
    /// First observation (ms) — the deterministic tie-break for §5 ordering.
    pub first_seen_ms: u64,
}

/// One crawler probe of a node (§4.2 `uptime_samples`). `in_sync` is the
/// §5 "at-sync-required" bit: a reachable-but-desynced node does NOT count as up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UptimeSample {
    pub node_key: String,
    pub at_ms: u64,
    /// Reachable AND at chain sync (headers within tolerance).
    pub in_sync: bool,
    /// Which crawler vantage took the sample (DE / JP …) — the evidence link.
    pub vantage: String,
    /// Evidence id (sample row id).
    pub evidence: String,
}

/// One validator/attestor epoch record (§4.2 `attestations`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationRow {
    pub validator_id: String,
    /// The DNS-attestation epoch this row covers.
    pub att_epoch: u64,
    /// Whether the validator attested this epoch.
    pub attested: bool,
    /// Whether a slashing/equivocation event landed on it in the window.
    pub slashed: bool,
    /// Evidence id (on-chain attestation / slash tx).
    pub evidence: String,
}

/// A GitHub bug-report event (§4.2 `gh_events`). Severity/first/fix are curated
/// by the triage step the github-sync collector wraps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhEvent {
    pub reporter_id: String,
    pub severity: Severity,
    /// First accepted report of this defect (not a duplicate).
    pub first_report: bool,
    /// A fix PR for it was accepted in the window (the §3.2 bonus).
    pub fix_pr_accepted: bool,
    /// Evidence id (GitHub issue / PR url).
    pub evidence: String,
}

/// A campaign/feedback or infra submission (§4.2 `submissions`). The collector
/// has already resolved the per-event cap / tier into `base_points` (§3.3/§3.4),
/// so scoring is a pure `pts_fixed`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub author_id: String,
    /// C3 (Verify) or C4 (Infra); Node-category fixed items use [`ChainFixed`].
    pub category: Category,
    pub base_points: u64,
    pub evidence: String,
}

/// A C1 fixed-value chain activity (IBD benchmark / partition drill, §3.1) —
/// scored via the deterministic core's `IbdBench` / `Drill` variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainFixed {
    pub author_id: String,
    pub kind: ChainFixedKind,
    pub evidence: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainFixedKind {
    /// IBD benchmark submission (§5.1 SLO).
    IbdBench,
    /// Partition-drill / load-test participation.
    Drill,
}

/// One **accepted, k=2 exact-matched** PALW replica slot (C5, ADR-0040 §16″).
///
/// A row exists only for work that already survived consensus: the leaf is in an accepted batch on
/// the selected chain, its DA object was produced, and A/B reproduced each other's
/// `output_commitment` and execution roots. The collector re-checks that off the finalized chain
/// rather than trusting a receipt handed to it — see [`crate::aggregate`].
///
/// The identity fields are the attribution chain ADR-0040 §16″ specifies:
/// `worker_credential_id` → provider bond → bond owner → registered MTP id (`owner_id`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmReplicaWork {
    /// When the job completed (ms). Evidence/audit only — never a scoring input.
    pub completed_at_ms: u64,
    /// The A/B pair this slot belongs to. Both slots of one job share it, and it is the key the
    /// "one identity, one point per pair" rule collapses on.
    pub pair_id: String,
    pub job_challenge: String,
    /// Global dedup key (ADR-0040 P1-9): one computation, one credit, forever.
    pub execution_nullifier: String,
    /// `txid:index` of the provider bond that backs this slot.
    pub provider_bond: String,
    pub worker_credential_id: String,
    /// 0 = replica A, 1 = replica B.
    pub replica_slot: u8,
    /// The registered MTP id the points land on (e.g. `gh:alice`).
    pub owner_id: String,
    /// Phase 1: always 1 (one accepted slot = one unit). Kept as a field so CU-proportional
    /// scoring is a later rules bump rather than a schema change.
    pub work_units: u64,
    /// Recorded as evidence only — not scored yet (A and B agreeing does not prove the value is
    /// honest; see `ScoringRules::c5_points_per_accepted_replica`).
    pub canonical_compute_units: u64,
    /// Evidence link (accepted block hash / leaf coordinate / DA root).
    pub evidence: String,
}

/// The whole §4.2 fact store, one Vec per table. Collectors append to it; the
/// aggregator reads it. Deliberately dependency-free (see module docs).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FactStore {
    pub identities: Vec<Identity>,
    pub nodes: Vec<NodeRecord>,
    pub uptime_samples: Vec<UptimeSample>,
    pub attestations: Vec<AttestationRow>,
    pub gh_events: Vec<GhEvent>,
    pub submissions: Vec<Submission>,
    pub chain_fixed: Vec<ChainFixed>,
    /// C5 accepted replica slots (ADR-0040 §16″).
    #[serde(default)]
    pub llm_replica_work: Vec<LlmReplicaWork>,
}

impl FactStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or re-affirm) an identity; idempotent on `id`.
    pub fn upsert_identity(&mut self, ident: Identity) {
        if let Some(existing) = self.identities.iter_mut().find(|i| i.id == ident.id) {
            existing.kind = ident.kind;
        } else {
            self.identities.push(ident);
        }
    }

    /// Register (or update) a node; idempotent on `node_key`.
    pub fn upsert_node(&mut self, node: NodeRecord) {
        if let Some(existing) = self.nodes.iter_mut().find(|n| n.node_key == node.node_key) {
            *existing = node;
        } else {
            self.nodes.push(node);
        }
    }

    /// All uptime samples for one node.
    pub fn samples_for(&self, node_key: &str) -> impl Iterator<Item = &UptimeSample> {
        self.uptime_samples.iter().filter(move |s| s.node_key == node_key)
    }

    /// Total row count across every table — a cheap "how much was collected" gauge.
    pub fn len(&self) -> usize {
        self.identities.len()
            + self.nodes.len()
            + self.uptime_samples.len()
            + self.attestations.len()
            + self.gh_events.len()
            + self.submissions.len()
            + self.chain_fixed.len()
            + self.llm_replica_work.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
