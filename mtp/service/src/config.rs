//! Service configuration (ADR-0038 D1, D2). The single knob that must never be
//! misconfigurable is the network family: the binary can only construct the
//! **testnet** address prefix, so there is no mainnet registration mode to fall
//! into. Everything else (role, paths, listen address) is ordinary deployment
//! config.

use kaspa_addresses::Prefix;
use misaka_mtp::Stage;
use std::path::PathBuf;

/// The role a service instance plays (ADR-0038 D2). A deployment runs one `Full`
/// node (cron + query-http + chain/github/forms collectors) and two `Vantage`
/// crawlers (DE, JP) that only feed uptime/node facts back to the full node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Cron + query-http + chain-indexer + github-sync + campaign-forms (the `.119` node).
    Full,
    /// A p2p-crawler vantage only (the DE / JP seeder hosts).
    Vantage,
}

/// The testnet networks in ADR-0027 D1 scope, with their ADR-0026 BPS stage coefficient.
///
/// **testnet-22 is the whole scope.** It replaced `testnet-21` at the 2026-08-02 static-audit
/// C-01/C-02 re-genesis (the PCPB clauses moved to fork-relative reads, which changes leaf
/// acceptance, and the DB version moved besides), which had replaced `testnet-20` at the
/// 2026-08-01 ADR-0045 D3-b re-genesis, which had replaced `testnet-200` at the 2026-07-30 re-genesis
/// (the IBD dead-loop incident): testnet-200 is deprecated, has no seeders, and cannot earn new
/// points — exactly as `testnet-10` was retired before it. No ledger state carries across a
/// re-genesis; the network NAME is the scope key, so old testnet-200/testnet-20 ledgers stay
/// verifiable under their own name while new epochs score only testnet-22. This is deployment scope (D1),
/// not a scoring rule: no `RULES_VERSION` bump, because no scored quantity changes — the 2→3
/// bump that accompanied the 10→200 migration was for the simultaneous Stage::B/C retirement,
/// not for the rename. (The BPS-ladder history: this list once carried `testnet-25/40/50`;
/// the block rate is fixed at 10 BPS (2 hash + 8 PALW replica), so those rungs are not coming.)
pub const NETWORKS: &[(&str, Stage)] = &[("testnet-22", Stage::A)];

/// The BPS stage for a scoped testnet network name, or `None` if out of scope
/// (e.g. a mainnet name — which by D1 can never reach the scorer anyway).
pub fn stage_for(network: &str) -> Option<Stage> {
    NETWORKS.iter().find(|(n, _)| *n == network).map(|(_, s)| *s)
}

/// Whole-service configuration. Constructed from CLI/env at [`crate::main`];
/// [`Self::prefix`] is hard-wired to testnet (D1).
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub role: Role,
    /// Network name this instance scores for (must be in [`NETWORKS`]).
    pub network: String,
    /// Root data directory: SQLite-equivalent fact store + published ledger archive.
    pub data_dir: PathBuf,
    /// Path to the dedicated MTP operator seed (D7); `Full` role only.
    pub operator_key_path: Option<String>,
    /// `query-http` listen address; `Full` role only.
    pub http_listen: Option<String>,
    /// In-repo maintainer allowlist for the label-actor gate (I-MTP-5).
    pub maintainer_allowlist: Vec<String>,
}

impl ServiceConfig {
    /// The one and only address prefix this service accepts — testnet, always.
    /// Not derived from `self`; a compile-time constant so no config path can
    /// turn on a mainnet registration mode (D1).
    pub const fn prefix(&self) -> Prefix {
        Prefix::Testnet
    }

    /// The BPS stage coefficient for [`Self::network`].
    pub fn stage(&self) -> Option<Stage> {
        stage_for(&self.network)
    }

    /// Directory holding the published, signed epoch ledger JSONL files + index.
    pub fn ledger_dir(&self) -> PathBuf {
        self.data_dir.join("points")
    }

    /// Directory holding the persistent (timed) fact store.
    pub fn store_dir(&self) -> PathBuf {
        self.data_dir.join("facts")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_always_testnet() {
        let cfg = ServiceConfig {
            role: Role::Full,
            network: "testnet-20".into(),
            data_dir: "/tmp/mtp".into(),
            operator_key_path: None,
            http_listen: None,
            maintainer_allowlist: vec![],
        };
        assert_eq!(cfg.prefix(), Prefix::Testnet, "D1: no mainnet mode exists");
    }

    #[test]
    fn stage_mapping_matches_adr_0026() {
        assert_eq!(stage_for("testnet-22"), Some(Stage::A));
        assert_eq!(stage_for("testnet-21"), None, "the deprecated re-genesis predecessor must not score new epochs");
        assert_eq!(stage_for("testnet-20"), None, "the deprecated re-genesis predecessor must not score new epochs");
        assert_eq!(stage_for("testnet-200"), None, "the deprecated re-genesis predecessor must not score new epochs");
        assert_eq!(stage_for("testnet-10"), None, "the retired network must not score new epochs");
        assert_eq!(stage_for("mainnet"), None, "out-of-scope names never score");
    }

    /// The BPS ladder is retired: the block rate is fixed at 10 BPS (2 hash + 8 PALW replica),
    /// so the escalation rungs must not creep back into scope. They were also never runnable —
    /// no consensus preset ever defined them.
    #[test]
    fn the_retired_bps_ladder_rungs_stay_out_of_scope() {
        for rung in ["testnet-25", "testnet-40", "testnet-50", "testnet-palw-40"] {
            assert_eq!(stage_for(rung), None, "{rung} is a retired ladder rung — it must not score");
        }
        assert_eq!(NETWORKS.len(), 1, "testnet-22 is the whole scope");
    }
}
