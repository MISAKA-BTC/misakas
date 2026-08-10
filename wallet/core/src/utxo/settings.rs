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
    coinbase_settlement_long_maturity_daa: 30_000,
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
            NetworkType::Testnet => match value.suffix {
                Some(10) => &TESTNET10_NETWORK_PARAMS,
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
    use kaspa_consensus_core::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};

    /// The wallet mirrors the consensus settlement knob per network; a drift between the two
    /// would make the wallet's "pending" disagree with the node's "won't relay", which is the
    /// exact confusion the mirror exists to prevent.
    #[test]
    fn settlement_knob_mirrors_consensus_params() {
        let t10 = &TESTNET10_NETWORK_PARAMS;
        assert_eq!(
            t10.coinbase_settlement_long_maturity_daa(),
            TESTNET_PARAMS.dns_params.as_ref().map_or(0, |p| p.coinbase_settlement_long_maturity_daa),
            "testnet-10 wallet params must mirror TESTNET_DNS_PARAMS"
        );
        assert_eq!(
            MAINNET_NETWORK_PARAMS.coinbase_settlement_long_maturity_daa(),
            MAINNET_PARAMS.dns_params.as_ref().map_or(0, |p| p.coinbase_settlement_long_maturity_daa),
            "mainnet wallet params must mirror PRODUCTION_DNS_PARAMS"
        );
    }
}
