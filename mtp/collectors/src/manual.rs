//! Manual awards — the operator-curated half of the Testnet Points Program.
//!
//! The **verification-required** categories are NOT auto-collected, because they
//! cannot be measured objectively from the chain or node telemetry — they need a
//! human review step:
//!   * **C2 bug reports** — severity triage, first-report vs duplicate, accepted-fix call;
//!   * **C3 verify / C4 infra** — judging a submitted verification task / infra contribution.
//!
//! So the auto pipeline ([`crate::aggregate::build_epoch_input`]) collects ONLY the
//! objectively chain-/telemetry-measurable categories (C1 node, validator uptime, the
//! chain-fixed benchmarks). Everything that needs review is added **by hand**: the
//! operator records each award with `misaka mtp award …`, which appends a
//! [`ManualAward`] line to a local JSONL. At epoch time the recompute loads the awards
//! for that `(epoch, network)` and merges the resulting [`ContributionEntry`] rows
//! alongside the auto facts, so the scoring + signed-ledger path is otherwise unchanged.
//!
//! The award file is a plain, append-only JSONL the operator owns and can audit — one
//! object per line, human-readable, diff-able, and replayable into the deterministic
//! `score_epoch`.

use std::io::Write;
use std::path::Path;

use misaka_mtp::{Contribution, ContributionEntry};
use serde::{Deserialize, Serialize};

/// One hand-curated award, scoped to a single `(epoch, network)`. Appended verbatim to
/// the manual-awards JSONL; the epoch recompute filters by `(epoch, network)` and merges
/// the resulting [`ContributionEntry`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManualAward {
    /// Epoch this award applies to — matched against `EpochWindow.epoch`.
    pub epoch: u64,
    /// Network this award applies to — matched against `EpochWindow.network`.
    pub network: String,
    /// Ledger id / actor (e.g. `"gh:alice"`).
    pub id: String,
    /// The awarded contribution. Only the verification-required kinds are meaningful here
    /// (`Bug` for C2, `Fixed { category: Verify | Infra, .. }` for C3 / C4); the CLI rejects
    /// the auto-only kinds (`Node` / `Validator` / chain-fixed).
    pub contribution: Contribution,
    /// Free-text note (reason / issue / PR link) recorded for the audit trail. Not scored.
    #[serde(default)]
    pub note: String,
}

/// Append one award as a JSON line to `path` (created if absent). Append-only — the
/// operator's running hand-curated ledger.
pub fn append_manual_award(path: impl AsRef<Path>, award: &ManualAward) -> std::io::Result<()> {
    let line = serde_json::to_string(award).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")
}

/// Load every award for `(epoch, network)` from the manual-awards JSONL and map them to
/// the [`ContributionEntry`] rows the recompute merges. A missing file yields no awards
/// (the auto-only path). A malformed line is a hard error, so one bad row cannot silently
/// drop the rest of the operator's ledger.
pub fn load_manual_awards(path: impl AsRef<Path>, epoch: u64, network: &str) -> Result<Vec<ContributionEntry>, String> {
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read manual-awards '{}': {e}", path.as_ref().display())),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let award: ManualAward = serde_json::from_str(line).map_err(|e| format!("manual-awards line {}: {e}", i + 1))?;
        if award.epoch == epoch && award.network == network {
            let evidence = if award.note.is_empty() { Vec::new() } else { vec![award.note.clone()] };
            out.push(ContributionEntry { id: award.id, contribution: award.contribution, evidence });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_mtp::{Category, Severity};

    fn bug(id: &str, epoch: u64) -> ManualAward {
        ManualAward {
            epoch,
            network: "testnet-palw-10".into(),
            id: id.into(),
            contribution: Contribution::Bug { severity: Severity::S1, first_report: true, fix_pr_accepted: false },
            note: "issue #42".into(),
        }
    }

    #[test]
    fn roundtrip_and_epoch_network_filter() {
        let dir = std::env::temp_dir().join("mtp-manual-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("awards-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);

        append_manual_award(&path, &bug("gh:alice", 12)).unwrap();
        append_manual_award(
            &path,
            &ManualAward {
                epoch: 12,
                network: "testnet-palw-10".into(),
                id: "gh:bob".into(),
                contribution: Contribution::Fixed { category: Category::Infra, base_points: 500 },
                note: String::new(),
            },
        )
        .unwrap();
        // A different epoch and a different network must NOT be picked up for (12, testnet-palw-10).
        append_manual_award(&path, &bug("gh:carol", 13)).unwrap();
        append_manual_award(&path, &ManualAward { network: "testnet-10".into(), ..bug("gh:dave", 12) }).unwrap();

        let loaded = load_manual_awards(&path, 12, "testnet-palw-10").unwrap();
        assert_eq!(loaded.len(), 2, "only alice(bug) + bob(infra) for (12, testnet-palw-10)");
        assert_eq!(loaded[0].id, "gh:alice");
        assert_eq!(loaded[1].id, "gh:bob");

        // Missing file → no awards (auto-only), not an error.
        assert!(load_manual_awards(dir.join("does-not-exist.jsonl"), 12, "testnet-palw-10").unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
