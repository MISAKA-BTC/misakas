//!
//! Wallet framework network parameters that control maturity
//! durations and other transaction related properties.
//!

use crate::imports::*;

#[derive(Debug)]
pub struct NetworkParams {
    pub coinbase_transaction_maturity_period_daa: AtomicU64,
    pub coinbase_transaction_stasis_period_daa: u64,
    pub user_transaction_maturity_period_daa: AtomicU64,
    pub additional_compound_transaction_mass: u64,
    /// DNS-accelerated coinbase settlement, mirroring the network's
    /// `DnsParams::coinbase_settlement_long_maturity_daa` (`0` = the network has no settlement
    /// and coinbase maturity behaves classically). When non-zero, a coinbase that has passed
    /// ordinary maturity stays **Pending** until the DNS confirmed anchor passes its block or
    /// the long fallback elapses — the same `coinbase_spend_settled` rule the node's mempool
    /// enforces, so the wallet's "pending" is exactly the node's "won't relay yet".
    pub coinbase_settlement_long_maturity_daa: u64,
    /// The DNS confirmed anchor's DAA score as last observed from the node
    /// (`get_dns_confirmation`), `0` = none observed. RUNTIME state, not a constant: the
    /// UtxoProcessor refreshes it at epoch cadence. Kept here because every maturity call site
    /// already receives `&NetworkParams`, so the anchor rides along without changing six call
    /// signatures — the same pattern as the runtime-settable maturity periods above.
    pub dns_confirmed_anchor_daa: AtomicU64,
}

impl NetworkParams {
    #[inline]
    pub fn coinbase_transaction_maturity_period_daa(&self) -> u64 {
        self.coinbase_transaction_maturity_period_daa.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn coinbase_transaction_stasis_period_daa(&self) -> u64 {
        self.coinbase_transaction_stasis_period_daa
    }

    #[inline]
    pub fn user_transaction_maturity_period_daa(&self) -> u64 {
        self.user_transaction_maturity_period_daa.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn additional_compound_transaction_mass(&self) -> u64 {
        self.additional_compound_transaction_mass
    }

    pub fn set_coinbase_transaction_maturity_period_daa(&self, value: u64) {
        self.coinbase_transaction_maturity_period_daa.store(value, Ordering::Relaxed);
    }

    pub fn set_user_transaction_maturity_period_daa(&self, value: u64) {
        self.user_transaction_maturity_period_daa.store(value, Ordering::Relaxed);
    }

    #[inline]
    pub fn coinbase_settlement_long_maturity_daa(&self) -> u64 {
        self.coinbase_settlement_long_maturity_daa
    }

    #[inline]
    pub fn dns_confirmed_anchor_daa(&self) -> u64 {
        self.dns_confirmed_anchor_daa.load(Ordering::Relaxed)
    }

    pub fn set_dns_confirmed_anchor_daa(&self, value: u64) {
        self.dns_confirmed_anchor_daa.store(value, Ordering::Relaxed);
    }
}

static MAINNET_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(1_000),
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: AtomicU64::new(100),
    additional_compound_transaction_mass: 100,
    coinbase_settlement_long_maturity_daa: 0,
    dns_confirmed_anchor_daa: AtomicU64::new(0),
});

static TESTNET10_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(1_000),
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: AtomicU64::new(100),
    additional_compound_transaction_mass: 100,
    // MIRRORS `TESTNET_DNS_PARAMS.coinbase_settlement_long_maturity_daa`, and the two must be
    // equal rather than merely similar: this value is fed to the SAME
    // `coinbase_spend_settled` the node's mempool calls (see `UtxoEntryReferenceExtension::
    // dns_settled`), so a wallet and a node built from one commit would otherwise disagree about
    // whether a coinbase is spendable — the wallet's "pending" contradicting the node's
    // "won't relay". `settlement_knob_mirrors_consensus_params` is what holds them together.
    //
    // The maturity periods above are NOT mirrors: they are the wallet's own, deliberately
    // stricter, display ladder. Only this field crosses into consensus's rule.
    //
    // It read 30_000 for eight days: `7c980a85` re-genesised testnet-10 and re-sized it to 3_000,
    // `9d69ddf4` re-sized it again to 600 for the 120 s block interval, and this side was left
    // behind both times. The test caught it on the first day; nothing ran it, because
    // `cargo test` stops at the first failing binary and the integration suite aborted ahead of
    // this crate.
    coinbase_settlement_long_maturity_daa: 600,
    dns_confirmed_anchor_daa: AtomicU64::new(0),
});

/// testnet-11 — the public PALW chain (ADR-0035 Decision 1). A separate static rather than a
/// second name for testnet-10's, because ADR-0035 keeps **both** lanes: *"t10 is untouched and
/// keeps its own lane (its wedge is its own workstream)"*. Two live networks that happen to agree
/// today are still two networks, and one shared static would mean a later change to either lane
/// silently moving the other.
///
/// The display ladder is copied from testnet-10 deliberately: t11 inherits t10's shape
/// (`TESTNET11_PARAMS` is `..TESTNET_PARAMS`, same 120 s interval and windows), and the ladder is
/// a product choice rather than a mirror, so inventing different numbers for it here would be a
/// product decision smuggled into a params fix.
static TESTNET11_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(1_000),
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: AtomicU64::new(100),
    additional_compound_transaction_mass: 100,
    // MIRRORS `TESTNET11_PARAMS.dns_params` — which is `TESTNET_DNS_PARAMS` today, since
    // `TESTNET11_PARAMS` is `..TESTNET_PARAMS` and overrides only genesis, seeders and the PoW
    // activations. The test reads it through `TESTNET11_PARAMS` rather than through
    // `TESTNET_PARAMS`, so the day t11 overrides `dns_params` the assertion follows on its own
    // instead of quietly checking the other network's number.
    coinbase_settlement_long_maturity_daa: 600,
    dns_confirmed_anchor_daa: AtomicU64::new(0),
});

static SIMNET_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(1_000),
    coinbase_transaction_stasis_period_daa: 500,
    user_transaction_maturity_period_daa: AtomicU64::new(100),
    additional_compound_transaction_mass: 0,
    coinbase_settlement_long_maturity_daa: 0,
    dns_confirmed_anchor_daa: AtomicU64::new(0),
});

static DEVNET_NETWORK_PARAMS: LazyLock<NetworkParams> = LazyLock::new(|| NetworkParams {
    coinbase_transaction_maturity_period_daa: AtomicU64::new(100),
    coinbase_transaction_stasis_period_daa: 50,
    user_transaction_maturity_period_daa: AtomicU64::new(10),
    additional_compound_transaction_mass: 0,
    coinbase_settlement_long_maturity_daa: 0,
    dns_confirmed_anchor_daa: AtomicU64::new(0),
});

impl NetworkParams {
    pub fn from(value: NetworkId) -> &'static NetworkParams {
        match value.network_type {
            NetworkType::Mainnet => &MAINNET_NETWORK_PARAMS,
            // The twin of `impl From<NetworkId> for Params` in `kaspa_consensus_core::config::params`
            // — same arms, same panic messages, and it is meant to stay that way. The defect this
            // arm fixes was not the panic: consensus gained `Some(11)` when ADR-0035 made
            // testnet-11 the public chain, and this side was never given it, so a wallet built
            // from this branch aborted on the network the branch exists to launch.
            //
            // The unsupported-suffix panic is left as it is ON PURPOSE. It is verbatim what
            // consensus does, and whether an unknown suffix should abort, error or fall back is a
            // decision for both twins together — a fallback here would be worse than the panic in
            // one specific way: the settlement field below must equal that network's OWN consensus
            // value, and a fallback would answer with another network's number rather than
            // admitting it does not know.
            NetworkType::Testnet => match value.suffix {
                Some(10) => &TESTNET10_NETWORK_PARAMS,
                Some(11) => &TESTNET11_NETWORK_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => &DEVNET_NETWORK_PARAMS,
            NetworkType::Simnet => &SIMNET_NETWORK_PARAMS,
        }
    }
}

/// Set the coinbase transaction maturity period DAA score for a given network.
/// This controls the DAA period after which the user transactions are considered mature
/// and the wallet subsystem emits the transaction maturity event.
pub fn set_coinbase_transaction_maturity_period_daa(network_id: &NetworkId, value: u64) {
    let network_params = NetworkParams::from(*network_id);
    if value <= network_params.coinbase_transaction_stasis_period_daa() {
        panic!(
            "Coinbase transaction maturity period must be greater than the stasis period of {} DAA",
            network_params.coinbase_transaction_stasis_period_daa()
        );
    }
    network_params.set_coinbase_transaction_maturity_period_daa(value);
}

/// Set the user transaction maturity period DAA score for a given network.
/// This controls the DAA period after which the user transactions are considered mature
/// and the wallet subsystem emits the transaction maturity event.
pub fn set_user_transaction_maturity_period_daa(network_id: &NetworkId, value: u64) {
    let network_params = NetworkParams::from(*network_id);
    if value == 0 {
        panic!("User transaction maturity period must be greater than 0");
    }
    network_params.set_user_transaction_maturity_period_daa(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::{MAINNET_PARAMS, Params, TESTNET_PARAMS, TESTNET11_PARAMS};

    fn consensus_settlement(params: &Params) -> u64 {
        params.dns_params.as_ref().map_or(0, |p| p.coinbase_settlement_long_maturity_daa)
    }

    /// The wallet mirrors the consensus settlement knob per network; a drift between the two
    /// would make the wallet's "pending" disagree with the node's "won't relay", which is the
    /// exact confusion the mirror exists to prevent.
    ///
    /// Each network is read through ITS OWN preset. `TESTNET11_PARAMS` inherits `dns_params` from
    /// `TESTNET_PARAMS` today, so checking t11 against `TESTNET_PARAMS` would pass and would keep
    /// passing after t11 overrode the field — checking the wrong network's number and calling it
    /// a mirror.
    #[test]
    fn settlement_knob_mirrors_consensus_params() {
        assert_eq!(
            TESTNET10_NETWORK_PARAMS.coinbase_settlement_long_maturity_daa(),
            consensus_settlement(&TESTNET_PARAMS),
            "testnet-10 wallet params must mirror its own preset's DnsParams"
        );
        assert_eq!(
            TESTNET11_NETWORK_PARAMS.coinbase_settlement_long_maturity_daa(),
            consensus_settlement(&TESTNET11_PARAMS),
            "testnet-11 wallet params must mirror its own preset's DnsParams"
        );
        assert_eq!(
            MAINNET_NETWORK_PARAMS.coinbase_settlement_long_maturity_daa(),
            consensus_settlement(&MAINNET_PARAMS),
            "mainnet wallet params must mirror PRODUCTION_DNS_PARAMS"
        );
    }

    /// **The two suffix tables must accept the same set**, compared by behaviour rather than by a
    /// copied list — because a copied list is what failed here.
    ///
    /// `NetworkParams::from` is the twin of `impl From<NetworkId> for Params`: same arms, same
    /// panic messages. Consensus gained `Some(11)` when ADR-0035 made testnet-11 the public chain
    /// and the wallet did not, so a wallet built from this branch aborted on the branch's own
    /// network — from `utxo::processor`, `tx::generator`, `storage::transaction::record` and the
    /// wasm bindings, i.e. essentially any wallet operation.
    ///
    /// The mirror test above cannot catch that: it names the networks it checks, so a network
    /// nobody added is a network nobody asserts. This asks both tables the same question over a
    /// range of suffixes and requires the answers to agree — supported on both sides, or refused
    /// on both.
    #[test]
    fn the_wallet_supports_exactly_the_testnet_suffixes_consensus_does() {
        // Panics are the expected answer for most of this range, so silence the hook while it
        // runs; a real failure is reported by the assertion, not by the backtrace.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let supported = |f: &dyn Fn(NetworkId)| -> Vec<u16> {
            (0u16..=20)
                .filter(|s| {
                    let id = NetworkId::with_suffix(NetworkType::Testnet, *s as u32);
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(id))).is_ok()
                })
                .collect()
        };
        let consensus = supported(&|id| {
            let _ = Params::from(id);
        });
        let wallet = supported(&|id| {
            let _ = NetworkParams::from(id);
        });
        std::panic::set_hook(hook);

        assert_eq!(consensus, wallet, "the wallet and consensus disagree about which testnet suffixes exist");
        // Not vacuous: if both tables somehow refused everything the comparison above would still
        // hold, and a wallet that supports no testnet is not the property wanted.
        assert!(consensus.contains(&10) && consensus.contains(&11), "expected testnet-10 and testnet-11, got {consensus:?}");
    }
}
