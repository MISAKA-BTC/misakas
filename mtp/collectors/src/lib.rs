//! MISAKA Testnet Points Program (MTP) — collectors / I/O layer (ADR-0027 §4).
//!
//! This crate is the **service-layer half** of MTP that the deterministic core
//! ([`misaka_mtp`]) deliberately excludes: the four collectors that gather raw
//! contribution facts (p2p-crawler ×2, chain-indexer, github-sync,
//! campaign-forms), the §4.2 fact store they fill, and the §5 Sybil-resistance
//! aggregation (per-ID node decrement + /24-or-ASN co-location cap) that folds
//! those facts into an [`misaka_mtp::EpochInput`].
//!
//! Boundary: nothing here scores or signs — it produces the `EpochInput` and
//! hands off to [`misaka_mtp::score_epoch`] + [`misaka_mtp::EpochLedger::sign`].
//! Keeping collection out of the core preserves the core's property that a
//! ledger is bit-reproducible from *published facts + published rules*: the
//! (trusted, single-operator) collection step is auditable via the evidence
//! links every fact carries, and the (trustless) scoring step is pure.
//!
//! The trait seam is [`collect::Collector`], with a [`collect::MockCollector`]
//! for offline pipeline tests — the same shape as `misaka-mil-provider`'s
//! `InferenceBackend` / `MockBackend`.

pub mod aggregate;
pub mod collect;
pub mod manual;
pub mod store;

pub use aggregate::{COLOCATION_CAP, build_epoch_input};
pub use collect::{
    AcceptedPalwLeaf, CampaignFormsCollector, ChainIndexerCollector, CollectError, Collector, EpochWindow, GithubSyncCollector,
    MockCollector, OwnerResolver, P2pCrawlerCollector, PalwNormalizeReport, PalwReplicaCollector, Rejected, ReplicaSlot, run_all,
};
pub use manual::{ManualAward, append_manual_award, load_manual_awards};
pub use store::{
    AttestationRow, ChainFixed, ChainFixedKind, FactStore, GhEvent, Identity, IdentityKind, LlmReplicaWork, NodeRecord, Submission,
    UptimeSample,
};

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_mtp::{Category, Rules, Severity, Stage, score_epoch};

    fn window() -> EpochWindow {
        EpochWindow {
            epoch: 12,
            range: ["2026-09-21T00:00:00Z".into(), "2026-09-28T00:00:00Z".into()],
            network: "testnet-10".into(),
            stage: Stage::A,
        }
    }

    fn node(key: &str, owner: &str, ip24: Option<[u8; 3]>, asn: Option<u32>, seen: u64) -> NodeRecord {
        NodeRecord {
            node_key: key.into(),
            owner_id: owner.into(),
            ip_v4_24: ip24,
            asn,
            geo_diverse: true,
            fast_follow: true,
            first_seen_ms: seen,
        }
    }

    fn sample(key: &str, in_sync: bool, ev: &str) -> UptimeSample {
        UptimeSample { node_key: key.into(), at_ms: 1, in_sync, vantage: "DE".into(), evidence: ev.into() }
    }

    #[test]
    fn collectors_fill_store_then_aggregate_to_scoreable_input() {
        let mut store = FactStore::new();
        let collectors: Vec<Box<dyn Collector>> = vec![
            Box::new(P2pCrawlerCollector {
                vantage: "DE".into(),
                nodes: vec![node("n1", "op:alice", Some([10, 0, 1]), Some(100), 1)],
                samples: vec![sample("n1", true, "s1"), sample("n1", true, "s2"), sample("n1", false, "s3")],
            }),
            Box::new(ChainIndexerCollector {
                attestations: vec![AttestationRow {
                    validator_id: "addr:bob".into(),
                    att_epoch: 5,
                    attested: true,
                    slashed: false,
                    evidence: "att1".into(),
                }],
                chain_fixed: vec![],
            }),
            Box::new(GithubSyncCollector {
                events: vec![GhEvent {
                    reporter_id: "gh:carol".into(),
                    severity: Severity::S1,
                    first_report: true,
                    fix_pr_accepted: false,
                    evidence: "gh#1".into(),
                }],
            }),
            Box::new(CampaignFormsCollector {
                submissions: vec![Submission {
                    author_id: "addr:dave".into(),
                    category: Category::Verify,
                    base_points: 30,
                    evidence: "form#1".into(),
                }],
            }),
        ];

        let report = run_all(&collectors, &window(), &mut store).unwrap();
        assert_eq!(report.len(), 4);
        assert!(!store.is_empty());

        // AUTO path: only the objectively-measurable categories are scored — node + validator.
        // The bug (gh:carol) and verify (addr:dave) facts sit in the store but are NOT
        // auto-collected (they need human review), so `build_epoch_input` ignores them.
        let auto = build_epoch_input(&window(), &store, &[]);
        assert_eq!(auto.contributions.len(), 2, "auto = node + validator only (bug/verify are manual)");
        assert!(
            !auto.contributions.iter().any(|c| matches!(c.contribution, misaka_mtp::Contribution::Bug { .. })),
            "bug is never auto-collected"
        );

        // MANUAL path: the operator hand-adds the reviewed bug + verify awards, and they merge in.
        let manual = vec![
            misaka_mtp::ContributionEntry {
                id: "gh:carol".into(),
                contribution: misaka_mtp::Contribution::Bug { severity: Severity::S1, first_report: true, fix_pr_accepted: false },
                evidence: vec!["gh#1".into()],
            },
            misaka_mtp::ContributionEntry {
                id: "addr:dave".into(),
                contribution: misaka_mtp::Contribution::Fixed { category: Category::Verify, base_points: 30 },
                evidence: vec!["form#1".into()],
            },
        ];
        let input = build_epoch_input(&window(), &store, &manual);
        // alice(node) + bob(validator) + carol(bug) + dave(verify) = 4 entries.
        assert_eq!(input.contributions.len(), 4);

        let ledger = score_epoch(&input, &Rules::default());
        // alice node: 100 base · u=2/3 · m_geo 1.5 · m_ver 1.2 · rank0 ·1 = 120_000 mpts (floor).
        let alice = ledger.scores.iter().find(|s| s.id == "op:alice").unwrap();
        assert_eq!(alice.c1, 120_000);
        // carol S1 first bug = 2000 pts in C2 (via the manual award).
        let carol = ledger.scores.iter().find(|s| s.id == "gh:carol").unwrap();
        assert_eq!(carol.c2, 2_000_000);
        // dave verify submission = 30 pts in C3 (via the manual award).
        let dave = ledger.scores.iter().find(|s| s.id == "addr:dave").unwrap();
        assert_eq!(dave.c3, 30_000);
    }

    #[test]
    fn colocation_cap_drops_the_third_node_in_a_24() {
        let mut store = FactStore::new();
        // Three nodes for the same owner in the SAME /24 → §5 caps at 2. No geo/ver
        // multipliers here, so the surviving score is pure rank decrement.
        for (i, key) in ["n1", "n2", "n3"].iter().enumerate() {
            let mut n = node(key, "op:sy", Some([10, 0, 9]), Some(7), i as u64);
            n.geo_diverse = false;
            n.fast_follow = false;
            store.upsert_node(n);
            store.uptime_samples.push(sample(key, true, &format!("s-{key}")));
        }
        let input = build_epoch_input(&window(), &store, &[]);
        // only 2 node entries survive the cap.
        let node_entries = input.contributions.iter().filter(|c| c.id == "op:sy").count();
        assert_eq!(node_entries, 2, "third node in the /24 is dropped");

        let ledger = score_epoch(&input, &Rules::default());
        let sy = ledger.scores.iter().find(|s| s.id == "op:sy").unwrap();
        // rank0 (×1.0) 100_000 + rank1 (×0.5) 50_000 = 150_000; the 3rd never counts.
        assert_eq!(sy.c1, 150_000);
    }

    #[test]
    fn keyless_nodes_are_fail_closed_capped_together() {
        let mut store = FactStore::new();
        // Three nodes with NEITHER /24 nor ASN, for THREE distinct owners. Without
        // the fail-closed bucket each would escape the cap; with it, the shared
        // keyless bucket (COLOCATION_CAP=2) drops the third.
        for (i, (key, owner)) in [("k1", "op:a"), ("k2", "op:b"), ("k3", "op:c")].iter().enumerate() {
            let mut n = node(key, owner, None, None, i as u64);
            n.geo_diverse = false;
            n.fast_follow = false;
            store.upsert_node(n);
            store.uptime_samples.push(sample(key, true, &format!("s-{key}")));
        }
        let input = build_epoch_input(&window(), &store, &[]);
        let node_entries =
            input.contributions.iter().filter(|c| matches!(c.contribution, misaka_mtp::Contribution::Node { .. })).count();
        assert_eq!(node_entries, 2, "unattributed nodes share one capped bucket (fail-closed)");
    }

    #[test]
    fn per_owner_rank_decrements_but_distinct_owners_do_not() {
        let mut store = FactStore::new();
        // Two owners, one node each, distinct /24 + ASN → both rank 0 (×1.0).
        store.upsert_node(node("a1", "op:a", Some([10, 0, 1]), Some(1), 1));
        store.upsert_node(node("b1", "op:b", Some([10, 0, 2]), Some(2), 1));
        store.uptime_samples.push(sample("a1", true, "sa"));
        store.uptime_samples.push(sample("b1", true, "sb"));
        let input = build_epoch_input(&window(), &store, &[]);
        let ledger = score_epoch(&input, &Rules::default());
        // both full 100·1.5·1.2 = 180_000 (no decrement across distinct owners).
        assert_eq!(ledger.scores.iter().find(|s| s.id == "op:a").unwrap().c1, 180_000);
        assert_eq!(ledger.scores.iter().find(|s| s.id == "op:b").unwrap().c1, 180_000);
    }

    #[test]
    fn aggregation_is_order_independent_and_reproducible() {
        let mut store = FactStore::new();
        store.upsert_node(node("n1", "op:a", Some([1, 2, 3]), Some(1), 2));
        store.upsert_node(node("n2", "op:a", Some([4, 5, 6]), Some(2), 1));
        store.uptime_samples.push(sample("n1", true, "s1"));
        store.uptime_samples.push(sample("n2", false, "s2"));

        let a = build_epoch_input(&window(), &store, &[]);
        // Reverse the raw table order; the deterministic node_order must undo it.
        store.nodes.reverse();
        store.uptime_samples.reverse();
        let b = build_epoch_input(&window(), &store, &[]);

        // The core's inputs_hash is order-independent, so the ledgers match.
        let la = score_epoch(&a, &Rules::default());
        let lb = score_epoch(&b, &Rules::default());
        assert_eq!(la, lb, "same facts → identical ledger regardless of table order");
    }

    #[test]
    fn slashed_validator_forfeits_and_partial_attestation_scales() {
        let mut store = FactStore::new();
        let ci = ChainIndexerCollector {
            attestations: vec![
                AttestationRow { validator_id: "v:x".into(), att_epoch: 1, attested: true, slashed: false, evidence: "e1".into() },
                AttestationRow { validator_id: "v:x".into(), att_epoch: 2, attested: false, slashed: false, evidence: "e2".into() },
                AttestationRow { validator_id: "v:y".into(), att_epoch: 1, attested: true, slashed: true, evidence: "e3".into() },
            ],
            chain_fixed: vec![],
        };
        ci.collect(&window(), &mut store).unwrap();
        let ledger = score_epoch(&build_epoch_input(&window(), &store, &[]), &Rules::default());
        // x attested 1/2 → 200_000 · 1/2 = 100_000.
        assert_eq!(ledger.scores.iter().find(|s| s.id == "v:x").unwrap().c1, 100_000);
        // y slashed → 0.
        assert_eq!(ledger.scores.iter().find(|s| s.id == "v:y").unwrap().c1, 0);
    }
}
