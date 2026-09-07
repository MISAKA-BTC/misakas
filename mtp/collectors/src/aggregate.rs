//! Aggregation (ADR-0027 §5, §4): fold the raw [`FactStore`] into the
//! deterministic core's [`EpochInput`], applying the §5 Sybil-resistance rules —
//! the per-ID node decrement `d_n` and the /24-or-ASN co-location cap — along the
//! way. Everything here is a pure, order-independent function of the store, so
//! two operators with the same facts build byte-identical input.

use crate::collect::EpochWindow;
use crate::store::{ChainFixedKind, FactStore, LlmReplicaWork, NodeRecord};
use misaka_mtp::rules::c5_points_collection_enabled;
use misaka_mtp::{Contribution, ContributionEntry, EpochInput};
use std::collections::BTreeMap;

/// §5 co-location cap: at most this many *counted* nodes may share a /24 prefix
/// or an ASN. Extra nodes in the same bucket are dropped before ranking.
pub const COLOCATION_CAP: usize = 2;

/// A node that survived the §5 co-location cap, with its per-owner rank assigned.
struct RankedNode<'a> {
    node: &'a NodeRecord,
    node_rank: usize,
}

/// Deterministic node order: earliest-seen first, `node_key` as the tie-break.
fn node_order(a: &NodeRecord, b: &NodeRecord) -> std::cmp::Ordering {
    a.first_seen_ms.cmp(&b.first_seen_ms).then_with(|| a.node_key.cmp(&b.node_key))
}

/// Apply the §5 /24-or-ASN cap, then assign the per-owner `d_n` rank. Nodes are
/// considered in deterministic [`node_order`]; a node is kept only if **every**
/// co-location key it exposes (/24 and/or ASN) is still under [`COLOCATION_CAP`].
/// Kept nodes are then ranked 0,1,2,… within each owner id (rank ≥ 4 scores 0 by
/// the core's `d_n` table, but we still emit it so the evidence is recorded).
///
/// Fail-closed on missing attribution (adversarial-review hardening): a node with
/// NEITHER a /24 nor an ASN exposes no co-location key, so it would otherwise
/// escape the cap entirely — "unknown location" must not read as "known-isolated"
/// in a Sybil control. All key-less nodes are bucketed together under one sentinel
/// and share the same [`COLOCATION_CAP`], so an operator can't farm unlimited
/// unattributed nodes. (In practice the /24 is crawler-observed from the TCP source
/// IP, so key-less nodes are rare, but the control fails safe regardless.)
fn rank_nodes(store: &FactStore) -> Vec<RankedNode<'_>> {
    let mut ordered: Vec<&NodeRecord> = store.nodes.iter().collect();
    ordered.sort_by(|a, b| node_order(a, b));

    // Co-location cap pass.
    let mut per_24: BTreeMap<[u8; 3], usize> = BTreeMap::new();
    let mut per_asn: BTreeMap<u32, usize> = BTreeMap::new();
    let mut keyless: usize = 0; // nodes exposing neither /24 nor ASN, capped together
    let mut kept: Vec<&NodeRecord> = Vec::new();
    for node in ordered {
        if node.ip_v4_24.is_none() && node.asn.is_none() {
            if keyless >= COLOCATION_CAP {
                continue; // fail-closed: unattributed nodes share one capped bucket
            }
            keyless += 1;
            kept.push(node);
            continue;
        }
        let over_24 = node.ip_v4_24.map(|k| *per_24.get(&k).unwrap_or(&0) >= COLOCATION_CAP).unwrap_or(false);
        let over_asn = node.asn.map(|k| *per_asn.get(&k).unwrap_or(&0) >= COLOCATION_CAP).unwrap_or(false);
        if over_24 || over_asn {
            continue; // this /24 or ASN already has COLOCATION_CAP counted nodes
        }
        if let Some(k) = node.ip_v4_24 {
            *per_24.entry(k).or_insert(0) += 1;
        }
        if let Some(k) = node.asn {
            *per_asn.entry(k).or_insert(0) += 1;
        }
        kept.push(node);
    }

    // Per-owner rank pass (kept is already in node_order, so ranks are stable).
    let mut per_owner: BTreeMap<&str, usize> = BTreeMap::new();
    kept.into_iter()
        .map(|node| {
            let rank = per_owner.entry(node.owner_id.as_str()).or_insert(0);
            let node_rank = *rank;
            *rank += 1;
            RankedNode { node, node_rank }
        })
        .collect()
}

/// C1 node contributions from uptime samples + the §5 rank (one entry per kept
/// node, attributed to its owner). `uptime_ok/total` is the at-sync-required
/// success rate (`in_sync` samples over all samples for the node).
fn node_contributions(store: &FactStore) -> Vec<ContributionEntry> {
    rank_nodes(store)
        .into_iter()
        .map(|rn| {
            let total = store.samples_for(&rn.node.node_key).count() as u64;
            let ok = store.samples_for(&rn.node.node_key).filter(|s| s.in_sync).count() as u64;
            let mut evidence: Vec<String> = store.samples_for(&rn.node.node_key).map(|s| s.evidence.clone()).collect();
            evidence.sort();
            ContributionEntry {
                id: rn.node.owner_id.clone(),
                contribution: Contribution::Node {
                    uptime_ok: ok,
                    uptime_total: total,
                    geo_diverse: rn.node.geo_diverse,
                    fast_follow: rn.node.fast_follow,
                    node_rank: rn.node_rank,
                },
                evidence,
            }
        })
        .collect()
}

/// C1 validator contributions: aggregate each validator's attestation rows into
/// `attested/total` epoch participation; any slash in the window forfeits it.
fn validator_contributions(store: &FactStore) -> Vec<ContributionEntry> {
    // (attested, total, slashed, evidence) per validator, in a BTreeMap for order.
    let mut agg: BTreeMap<&str, (u64, u64, bool, Vec<String>)> = BTreeMap::new();
    for a in &store.attestations {
        let e = agg.entry(a.validator_id.as_str()).or_insert((0, 0, false, Vec::new()));
        e.1 += 1;
        if a.attested {
            e.0 += 1;
        }
        e.2 |= a.slashed;
        e.3.push(a.evidence.clone());
    }
    agg.into_iter()
        .map(|(id, (attested, total, slashed, mut evidence))| {
            evidence.sort();
            ContributionEntry {
                id: id.to_string(),
                contribution: Contribution::Validator { attested_epochs: attested, total_epochs: total, slashed },
                evidence,
            }
        })
        .collect()
}

/// C1 fixed chain activities (IBD bench / drill), one entry per row.
fn chain_fixed_contributions(store: &FactStore) -> Vec<ContributionEntry> {
    store
        .chain_fixed
        .iter()
        .map(|c| ContributionEntry {
            id: c.author_id.clone(),
            contribution: match c.kind {
                ChainFixedKind::IbdBench => Contribution::IbdBench,
                ChainFixedKind::Drill => Contribution::Drill,
            },
            evidence: vec![c.evidence.clone()],
        })
        .collect()
}

/// C2 bug contributions, one entry per triaged gh event.
///
/// NOT part of the auto pipeline any more: C2 bug reports need human triage (severity,
/// first-report vs duplicate, accepted-fix), so they are added by hand via
/// [`crate::manual`] (`misaka mtp award`) and merged into `build_epoch_input`'s `manual`
/// argument. Kept for the `Contribution::Bug` shape (and in case a future automated,
/// pre-triaged gh feed is wired back in).
#[allow(dead_code)]
fn bug_contributions(store: &FactStore) -> Vec<ContributionEntry> {
    store
        .gh_events
        .iter()
        .map(|e| ContributionEntry {
            id: e.reporter_id.clone(),
            contribution: Contribution::Bug { severity: e.severity, first_report: e.first_report, fix_pr_accepted: e.fix_pr_accepted },
            evidence: vec![e.evidence.clone()],
        })
        .collect()
}

/// C3/C4 fixed submissions, one entry per row (base points already tier-resolved).
///
/// NOT part of the auto pipeline any more: C3 verify / C4 infra submissions need a human
/// review call, so they are added by hand via [`crate::manual`] (`misaka mtp award`) and
/// merged into `build_epoch_input`'s `manual` argument. Kept for the shape / potential
/// re-enable.
#[allow(dead_code)]
fn submission_contributions(store: &FactStore) -> Vec<ContributionEntry> {
    store
        .submissions
        .iter()
        .map(|s| ContributionEntry {
            id: s.author_id.clone(),
            contribution: Contribution::Fixed { category: s.category, base_points: s.base_points },
            evidence: vec![s.evidence.clone()],
        })
        .collect()
}

/// C5 PALW replica contributions (ADR-0040 §16″), with every §5-equivalent defence applied here so
/// the scorer stays a pure count.
///
/// Three filters, in this order, and each one exists because of a specific way points could
/// otherwise be farmed:
///
/// 1. **Nullifier dedup (P1-9).** One `execution_nullifier` = one computation, credited once, no
///    matter how many times it is presented. Re-submitting yesterday's job is the cheapest possible
///    attack and the only defence is a global key.
/// 2. **One point per identity per pair.** Receipt-v3 already forbids A and B sharing a
///    `worker_credential_id`, but nothing stops one operator holding two credentials and two bonds
///    and self-matching. If both slots of a pair resolve to the SAME `owner_id`, that identity earns
///    **1** point for the pair, not 2 — so pointing a second GPU at your own job earns what one job
///    is worth. Distinct owners each earn their slot, which is the honest case.
/// 3. **Gate.** Nothing is emitted while [`c5_points_collection_enabled`] is `false`.
///
/// Deterministic throughout: rows are keyed and folded through `BTreeMap`s, so two operators with the
/// same facts emit byte-identical entries regardless of collection order.
///
/// Note what is NOT here: no reorg check. The caller must collect from a **finality-buried** selected
/// chain (see [`crate::collect`]), because a leaf that later falls out of the chain must never have
/// been scored — a point outliving its block is the ledger's version of a ghost.
fn llm_replica_contributions(store: &FactStore) -> Vec<ContributionEntry> {
    if !c5_points_collection_enabled() {
        return Vec::new();
    }
    // (1) Global dedup by execution nullifier. Ties break on the full row key so the survivor is
    // deterministic rather than collection-order dependent.
    let mut by_nullifier: BTreeMap<&str, &LlmReplicaWork> = BTreeMap::new();
    for w in &store.llm_replica_work {
        by_nullifier
            .entry(w.execution_nullifier.as_str())
            .and_modify(|kept| {
                let key = |x: &LlmReplicaWork| (x.pair_id.clone(), x.replica_slot, x.owner_id.clone());
                if key(w) < key(kept) {
                    *kept = w;
                }
            })
            .or_insert(w);
    }

    // (2) Collapse to one credit per (pair, identity), then total per identity.
    let mut credited: BTreeMap<(&str, &str), u64> = BTreeMap::new(); // (pair_id, owner_id) -> units
    let mut evidence: BTreeMap<&str, Vec<String>> = BTreeMap::new(); // owner_id -> evidence
    for w in by_nullifier.values() {
        let slot = credited.entry((w.pair_id.as_str(), w.owner_id.as_str())).or_insert(0);
        // max, not sum: a pair yields at most one credit to any single identity.
        *slot = (*slot).max(w.work_units);
        evidence.entry(w.owner_id.as_str()).or_default().push(w.evidence.clone());
    }

    let mut units: BTreeMap<&str, u64> = BTreeMap::new();
    for ((_, owner), u) in credited {
        *units.entry(owner).or_insert(0) = units.get(owner).copied().unwrap_or(0).saturating_add(u);
    }

    units
        .into_iter()
        .filter(|&(_, work_units)| work_units > 0)
        .map(|(owner, work_units)| {
            let mut ev = evidence.remove(owner).unwrap_or_default();
            ev.sort();
            ev.dedup();
            ContributionEntry { id: owner.to_string(), contribution: Contribution::LlmReplica { work_units }, evidence: ev }
        })
        .collect()
}

/// Fold the whole store into the deterministic core's [`EpochInput`] for
/// `window`. The result is fed straight into [`misaka_mtp::score_epoch`]; its
/// `inputs_hash` is order-independent, so the entry order here is not consensus-
/// critical, but it is stable (node → validator → chain-fixed → manual).
///
/// Only the **objectively measurable** categories are auto-collected here: C1 node
/// operation, validator uptime, and the chain-fixed benchmarks. The
/// **verification-required** categories — C2 bug reports and C3/C4 verify/infra — are
/// NOT auto-collected; they are supplied by the operator via `manual` (hand-curated with
/// `misaka mtp award`, loaded by [`crate::manual::load_manual_awards`]). This keeps the
/// automatic ledger free of anything that needs human review, and lets the operator add
/// those points by hand after their own verification.
///
/// **Contract: the `store` must hold facts for exactly `window` and no other
/// epoch.** This function counts every fact in the store (it does not itself
/// filter by `window.range`), so the per-epoch cron MUST collect into a *fresh*
/// [`FactStore`] each run (see [`crate::collect::run_all`], which appends into a
/// caller-owned store). Reusing an accumulating store across epochs would
/// double-count prior epochs' facts — the caller owns this scoping. The `manual`
/// slice must likewise already be filtered to this `(epoch, network)`.
pub fn build_epoch_input(window: &EpochWindow, store: &FactStore, manual: &[ContributionEntry]) -> EpochInput {
    let mut contributions = Vec::new();
    contributions.extend(node_contributions(store));
    contributions.extend(validator_contributions(store));
    contributions.extend(chain_fixed_contributions(store));
    // C5 PALW replica work — auto-collected, because it is fully provable from the finalized
    // selected chain (accepted leaf + k=2 match + DA object) and needs no human call.
    contributions.extend(llm_replica_contributions(store));
    // Verification-required categories (C2 bug, C3/C4 verify/infra) are hand-added only.
    contributions.extend(manual.iter().cloned());

    EpochInput {
        epoch: window.epoch,
        range: window.range.clone(),
        network: window.network.clone(),
        stage: window.stage,
        contributions,
    }
}

#[cfg(test)]
mod c5_tests {
    use super::*;

    fn work(pair: &str, slot: u8, owner: &str, nullifier: &str) -> LlmReplicaWork {
        LlmReplicaWork {
            completed_at_ms: 1_700_000_000_000,
            pair_id: pair.into(),
            job_challenge: format!("challenge-{pair}"),
            execution_nullifier: nullifier.into(),
            provider_bond: format!("bond-{owner}:0"),
            worker_credential_id: format!("cred-{owner}-{slot}"),
            replica_slot: slot,
            owner_id: owner.into(),
            work_units: 1,
            canonical_compute_units: 781_556,
            evidence: format!("block-{pair}-{slot}"),
        }
    }

    fn units(store: &FactStore) -> BTreeMap<String, u64> {
        llm_replica_contributions(store)
            .into_iter()
            .map(|e| match e.contribution {
                Contribution::LlmReplica { work_units } => (e.id, work_units),
                other => panic!("C5 collector emitted a non-C5 contribution: {other:?}"),
            })
            .collect()
    }

    /// The honest case: two distinct providers each get their slot.
    #[test]
    fn distinct_owners_each_earn_their_slot() {
        let store = FactStore {
            llm_replica_work: vec![work("p1", 0, "gh:alice", "n-a"), work("p1", 1, "gh:bob", "n-b")],
            ..FactStore::default()
        };
        assert_eq!(units(&store), BTreeMap::from([("gh:alice".to_string(), 1), ("gh:bob".to_string(), 1)]));
    }

    /// Self-matching: one operator runs BOTH slots under two credentials/bonds. Receipt-v3 permits
    /// that (the credentials differ); MTP must still pay for one job, not two.
    #[test]
    fn one_identity_holding_both_slots_earns_one_point_for_the_pair() {
        let store = FactStore {
            llm_replica_work: vec![work("p1", 0, "gh:alice", "n-a"), work("p1", 1, "gh:alice", "n-b")],
            ..FactStore::default()
        };
        assert_eq!(units(&store), BTreeMap::from([("gh:alice".to_string(), 1)]), "a pair is worth at most 1 to one id");
    }

    /// …but the same identity working several DIFFERENT pairs is real work and accumulates.
    #[test]
    fn the_same_identity_accumulates_across_distinct_pairs() {
        let store = FactStore {
            llm_replica_work: vec![
                work("p1", 0, "gh:alice", "n-1"),
                work("p2", 0, "gh:alice", "n-2"),
                work("p3", 1, "gh:alice", "n-3"),
            ],
            ..FactStore::default()
        };
        assert_eq!(units(&store), BTreeMap::from([("gh:alice".to_string(), 3)]));
    }

    /// P1-9: replaying one computation under new pair/bond labels earns nothing extra.
    #[test]
    fn a_replayed_execution_nullifier_is_credited_once() {
        let store = FactStore {
            llm_replica_work: vec![
                work("p1", 0, "gh:alice", "same-nullifier"),
                work("p2", 0, "gh:alice", "same-nullifier"),
                work("p3", 1, "gh:alice", "same-nullifier"),
            ],
            ..FactStore::default()
        };
        assert_eq!(units(&store), BTreeMap::from([("gh:alice".to_string(), 1)]), "one computation, one credit");
    }

    /// Two operators with the same facts must produce byte-identical input.
    #[test]
    fn collection_order_does_not_change_the_result() {
        let rows = vec![work("p2", 1, "gh:bob", "n-4"), work("p1", 0, "gh:alice", "n-1"), work("p1", 1, "gh:bob", "n-2")];
        let forward = FactStore { llm_replica_work: rows.clone(), ..FactStore::default() };
        let mut reversed_rows = rows;
        reversed_rows.reverse();
        let reversed = FactStore { llm_replica_work: reversed_rows, ..FactStore::default() };
        assert_eq!(units(&forward), units(&reversed));
        let a = serde_json::to_string(&llm_replica_contributions(&forward)).unwrap();
        let b = serde_json::to_string(&llm_replica_contributions(&reversed)).unwrap();
        assert_eq!(a, b, "entries (including evidence order) must be canonical");
    }

    /// C5 rows must reach the epoch input through the normal build path, tagged as C5.
    #[test]
    fn c5_reaches_the_epoch_input_as_the_llm_category() {
        use misaka_mtp::Category;
        let store = FactStore {
            llm_replica_work: vec![work("p1", 0, "gh:alice", "n-a"), work("p1", 1, "gh:bob", "n-b")],
            ..FactStore::default()
        };
        let window = EpochWindow {
            epoch: 1,
            range: ["2026-08-01T00:00:00Z".into(), "2026-08-08T00:00:00Z".into()],
            network: "testnet-20".into(),
            stage: misaka_mtp::Stage::A,
        };
        let input = build_epoch_input(&window, &store, &[]);
        let c5: Vec<_> = input.contributions.iter().filter(|e| e.contribution.category() == Category::Llm).collect();
        assert_eq!(c5.len(), 2, "both providers present");
        assert!(c5.iter().all(|e| !e.evidence.is_empty()), "every point cites its evidence (§3)");
    }
}

#[cfg(test)]
mod c5_end_to_end {
    use super::*;
    use misaka_mtp::{Category, Rules, Stage, score_epoch};

    /// The whole C5 path in one assertion: two providers complete one verified job, the collector
    /// credits each slot once, and `score_epoch` puts the points in the C5 column of the ledger.
    #[test]
    fn a_verified_pair_reaches_the_ledgers_c5_column() {
        let row = |pair: &str, slot: u8, owner: &str, nul: &str| crate::store::LlmReplicaWork {
            completed_at_ms: 1_700_000_000_000,
            pair_id: pair.into(),
            job_challenge: "challenge".into(),
            execution_nullifier: nul.into(),
            provider_bond: format!("bond-{owner}:0"),
            worker_credential_id: format!("cred-{owner}"),
            replica_slot: slot,
            owner_id: owner.into(),
            work_units: 1,
            canonical_compute_units: 781_556,
            evidence: format!("block-{pair}-{slot}"),
        };
        let store = FactStore {
            llm_replica_work: vec![row("p1", 0, "gh:alice", "n-a"), row("p1", 1, "gh:bob", "n-b")],
            ..FactStore::default()
        };
        let window = EpochWindow {
            epoch: 7,
            range: ["2026-08-01T00:00:00Z".into(), "2026-08-08T00:00:00Z".into()],
            network: "testnet-20".into(),
            stage: Stage::A,
        };
        let ledger = score_epoch(&build_epoch_input(&window, &store, &[]), &Rules::default());

        let alice = ledger.scores.iter().find(|s| s.id == "gh:alice").expect("alice scored");
        assert_eq!(alice.c5, misaka_mtp::POINT, "1 verified slot = 1 point, in C5");
        assert_eq!((alice.c1, alice.c2, alice.c3, alice.c4), (0, 0, 0, 0), "C5 work must not leak into another category");
        assert!(!alice.evidence.is_empty(), "the point cites the block it came from");
        let bob = ledger.scores.iter().find(|s| s.id == "gh:bob").expect("bob scored");
        assert_eq!(bob.c5, misaka_mtp::POINT);
        assert_eq!(Category::Llm.index(), 4, "C5 is the ledger's 5th column");
    }
}
