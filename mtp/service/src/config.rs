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
/// **testnet-200 is the whole scope.** The previous public network, `testnet-10`,
/// is retired and cannot earn new points. This list previously also carried `testnet-25`,
/// `testnet-40` and `testnet-50`, the rungs of a planned BPS-escalation ladder. The block rate
/// is now fixed at 10 BPS (2 hash + 8 PALW replica), so those rungs are not coming — and none
/// of the three ever existed as a consensus preset, so the scorer was scoping networks that
/// could not be run. Retired together with `Stage::B`/`Stage::C` (see `mtp::rules::Stage`);
/// `RULES_VERSION` bumped to 2 for that change and to 3 for the public-network migration.
pub const NETWORKS: &[(&str, Stage)] = &[("testnet-200", Stage::A)];

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
            network: "testnet-200".into(),
            data_dir: "/tmp/mtp".into(),
            operator_key_path: None,
            http_listen: None,
            maintainer_allowlist: vec![],
        };
        assert_eq!(cfg.prefix(), Prefix::Testnet, "D1: no mainnet mode exists");
    }

    #[test]
    fn stage_mapping_matches_adr_0026() {
        assert_eq!(stage_for("testnet-200"), Some(Stage::A));
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
        assert_eq!(NETWORKS.len(), 1, "testnet-200 is the whole scope");
    }
}
