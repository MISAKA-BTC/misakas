//! MISAKA Testnet Points Program (MTP) — ADR-0027.
//!
//! Off-chain, consensus-neutral. This crate is the **deterministic core**: it
//! turns collected contribution facts into an ML-DSA-87-signed epoch ledger, and
//! settles the accumulated points into an MSK allocation at TGE. All scoring is
//! integer-only (points in milli-points, multipliers as exact rationals) so a
//! ledger is bit-reproducible from published facts + published rules.
//!
//! The I/O collectors (p2p-crawler, chain-indexer, github-sync, campaign-forms;
//! design §4.1) are a separate service layer that feeds [`EpochInput`]; they are
//! NOT in this crate — this is the auditable pure function they wrap.

pub mod ledger;
pub mod registry;
pub mod rules;
pub mod score;
pub mod settle;

use std::collections::BTreeMap;

pub use ledger::{EpochLedger, ScoreRow};
pub use registry::{Registration, RegistrationError, verify_claim, verify_registration};
pub use rules::{AllocationRules, Category, MilliPoints, POINT, Rules, ScoringRules, Severity, Stage};
pub use score::{Contribution, pts_bug, pts_fixed, pts_node, pts_validator, scale};
pub use settle::{CategoryPoints, Settlement, settle, vesting_split};

use kaspa_hashes::blake2b_512_keyed;
use serde::{Deserialize, Serialize};

/// Domain-separation contexts (disjoint from the consensus tx/att/unbond contexts
/// and from the MIL contexts). All ≤ 255 bytes (ML-DSA context cap).
pub const MTP_RULES_CONTEXT: &[u8] = b"misaka-mtp-v1/rules";
pub const MTP_INPUTS_CONTEXT: &[u8] = b"misaka-mtp-v1/inputs";
pub const MTP_LEDGER_CONTEXT: &[u8] = b"misaka-mtp-v1/ledger/mldsa87";
pub const MTP_REGISTER_CONTEXT: &[u8] = b"misaka-mtp-v1/register/mldsa87";
pub const MTP_CLAIM_CONTEXT: &[u8] = b"misaka-mtp-v1/claim/mldsa87";

/// One collected contribution attributed to an identity, with its evidence link(s).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContributionEntry {
    pub id: String,
    pub contribution: Contribution,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// All facts for one epoch — the input the collectors produce and `score_epoch`
/// consumes. `stage` sets the BPS coefficient applied to every contribution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochInput {
    pub epoch: u64,
    pub range: [String; 2],
    pub network: String,
    pub stage: Stage,
    pub contributions: Vec<ContributionEntry>,
}

impl EpochInput {
    /// `blake2b_512_keyed(MTP_INPUTS_CONTEXT, canonical facts)` — order-independent
    /// (entries sorted by their JSON form) so the same facts hash the same.
    pub fn inputs_hash(&self) -> kaspa_hashes::Hash64 {
        let mut entries: Vec<String> =
            self.contributions.iter().map(|e| serde_json::to_string(e).expect("entry JSON is infallible")).collect();
        entries.sort();
        let canonical = serde_json::to_vec(&(self.epoch, &self.network, &self.stage, &self.range, &entries))
            .expect("canonical JSON is infallible");
        blake2b_512_keyed(MTP_INPUTS_CONTEXT, &canonical)
    }
}

/// Score one epoch into an (unsigned) ledger: pure function of facts × rules.
/// Points aggregate per identity per category; the scores are sorted by identity
/// so the ledger is canonical. Sign the result with [`EpochLedger::sign`].
pub fn score_epoch(input: &EpochInput, rules: &Rules) -> EpochLedger {
    let mut agg: BTreeMap<String, (crate::settle::CategoryPoints, Vec<String>)> = BTreeMap::new();
    for entry in &input.contributions {
        let cat = entry.contribution.category().index();
        let pts = entry.contribution.points(rules, input.stage);
        let slot = agg.entry(entry.id.clone()).or_insert(([0; Category::ALL.len()], Vec::new()));
        slot.0[cat] = slot.0[cat].saturating_add(pts);
        slot.1.extend(entry.evidence.iter().cloned());
    }
    let scores: Vec<ScoreRow> = agg
        .into_iter()
        .map(|(id, (p, evidence))| ScoreRow { id, c1: p[0], c2: p[1], c3: p[2], c4: p[3], c5: p[4], evidence })
        .collect();

    EpochLedger {
        epoch: input.epoch,
        range: input.range.clone(),
        network: input.network.clone(),
        rules_hash: faster_hex::hex_string(&rules.rules_hash().as_bytes()),
        inputs_hash: faster_hex::hex_string(&input.inputs_hash().as_bytes()),
        scores,
        sig_mldsa87: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, c: Contribution) -> ContributionEntry {
        ContributionEntry { id: id.into(), contribution: c, evidence: vec![] }
    }

    fn sample_input() -> EpochInput {
        EpochInput {
            epoch: 12,
            range: ["2026-09-21T00:00:00Z".into(), "2026-09-28T00:00:00Z".into()],
            network: "testnet-10".into(),
            stage: Stage::A,
            contributions: vec![
                entry(
                    "gh:alice",
                    Contribution::Node { uptime_ok: 100, uptime_total: 100, geo_diverse: true, fast_follow: true, node_rank: 0 },
                ),
                entry("gh:alice", Contribution::Bug { severity: Severity::S1, first_report: true, fix_pr_accepted: false }),
                entry("gh:bob", Contribution::Validator { attested_epochs: 1, total_epochs: 1, slashed: false }),
            ],
        }
    }

    #[test]
    fn score_epoch_aggregates_and_is_reproducible() {
        let rules = Rules::default();
        let input = sample_input();
        let l1 = score_epoch(&input, &rules);
        let l2 = score_epoch(&input, &rules);
        assert_eq!(l1, l2, "same facts + rules → identical ledger");
        // sorted by id: alice before bob.
        assert_eq!(l1.scores[0].id, "gh:alice");
        // alice: node 100·1.5·1.2 = 180 pts (180_000 mpts), bug S1 = 2000 pts.
        assert_eq!(l1.scores[0].c1, 180_000);
        assert_eq!(l1.scores[0].c2, 2_000_000);
        // bob: validator 200 pts in C1.
        assert_eq!(l1.scores[1].c1, 200_000);
        assert_eq!(l1.scores[1].c2, 0);
    }

    #[test]
    fn reordering_facts_does_not_change_the_ledger() {
        let rules = Rules::default();
        let mut a = sample_input();
        let l_a = score_epoch(&a, &rules);
        a.contributions.reverse();
        let l_b = score_epoch(&a, &rules);
        assert_eq!(l_a, l_b, "aggregation + inputs_hash are order-independent");
    }

    #[test]
    fn full_pipeline_score_sign_settle() {
        let rules = Rules::default();
        let mut ledger = score_epoch(&sample_input(), &rules);
        let op = kaspa_pq_validator_core::ValidatorKey::from_seed([9; 32]);
        ledger.sign(&op);
        assert!(ledger.verify(op.public_key()));

        // settle a 1000-sompi pool over the epoch's category points.
        let ids_points: Vec<(String, CategoryPoints)> =
            ledger.scores.iter().map(|s| (s.id.clone(), [s.c1, s.c2, s.c3, s.c4, s.c5])).collect();
        let s = settle(1000, &crate::AllocationRules::default(), &ids_points);
        assert_eq!(s.rewards.iter().map(|(_, r)| r).sum::<u64>() + s.ecosystem_remainder, 1000, "lossless");
    }
}
