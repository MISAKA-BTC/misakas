use crate::mempool::attestation::AttestationMempoolPolicy;
use kaspa_consensus_core::constants::TX_VERSION;

pub(crate) const DEFAULT_MAXIMUM_TRANSACTION_COUNT: usize = 1_000_000;
pub(crate) const DEFAULT_MEMPOOL_SIZE_LIMIT: usize = 1_000_000_000;
pub(crate) const DEFAULT_MAXIMUM_BUILD_BLOCK_TEMPLATE_ATTEMPTS: u64 = 5;

pub(crate) const DEFAULT_TRANSACTION_EXPIRE_INTERVAL_SECONDS: u64 = 24 * 60 * 60;
pub(crate) const DEFAULT_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS: u64 = 60;
pub(crate) const DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_INTERVAL_SECONDS: u64 = 120;
pub(crate) const DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS: u64 = 10;
pub(crate) const DEFAULT_ORPHAN_EXPIRE_INTERVAL_SECONDS: u64 = 60;
pub(crate) const DEFAULT_ORPHAN_EXPIRE_SCAN_INTERVAL_SECONDS: u64 = 10;

pub(crate) const DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_MASS: u64 = 100_000;
pub(crate) const DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_COUNT: u64 = 500;

/// DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
///
/// This is the upstream Kaspa base rate, used by `Config::build_default` (and the mempool unit-test
/// fixtures calibrated to it). The PRODUCTION kaspa-pq node raises it ×10 to
/// [`PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE`] via `MiningManager::new_with_extended_config`.
pub(crate) const DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;

/// kaspa-pq production relay fee rate: **10× the upstream Kaspa rate (1000 → 10_000)**. An ML-DSA-87
/// P2PKH transaction's compute mass is ≈ 10× a secp256k1 transaction's (its ~7.3 KB spend input — a
/// 4628-byte signature + a 2592-byte public key — plus `mass_per_sig_op = 10_000`), so this ×10
/// relay rate makes the effective minimum fee of a kaspa-pq transaction ≈ **100× a Kaspa
/// transaction's** — the intended reconciliation ("辻褄合わせ") for the ~72×-larger post-quantum
/// signature, and the fee base that funds validator/worker rewards once the 20-year block subsidy
/// reaches 0 (the §F fee split keeps routing fees worker 90% / validator 10%). Relay/mempool policy
/// only — NOT consensus, so no genesis change. The production node applies it; the test path keeps
/// the upstream base so the mempool unit fixtures stay calibrated. Calibratable.
pub const PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE: u64 = 10_000;

/// Standard transaction version range might be different from what consensus accepts, therefore
/// we define separate values in mempool.
/// However, currently there's exactly one transaction version, so mempool accepts the same version
/// as consensus.
pub(crate) const DEFAULT_MINIMUM_STANDARD_TRANSACTION_VERSION: u16 = TX_VERSION;
pub(crate) const DEFAULT_MAXIMUM_STANDARD_TRANSACTION_VERSION: u16 = TX_VERSION;

#[derive(Clone, Debug)]
pub struct Config {
    pub maximum_transaction_count: usize,
    pub mempool_size_limit: usize,
    pub maximum_build_block_template_attempts: u64,
    pub transaction_expire_interval_daa_score: u64,
    pub transaction_expire_scan_interval_daa_score: u64,
    pub transaction_expire_scan_interval_milliseconds: u64,
    pub accepted_transaction_expire_interval_daa_score: u64,
    pub accepted_transaction_expire_scan_interval_daa_score: u64,
    pub accepted_transaction_expire_scan_interval_milliseconds: u64,
    pub orphan_expire_interval_daa_score: u64,
    pub orphan_expire_scan_interval_daa_score: u64,
    pub maximum_orphan_transaction_mass: u64,
    pub maximum_orphan_transaction_count: u64,
    pub accept_non_standard: bool,
    pub maximum_mass_per_block: u64,
    pub minimum_relay_transaction_fee: u64,
    pub minimum_standard_transaction_version: u16,
    pub maximum_standard_transaction_version: u16,
    pub network_blocks_per_second: u64,
    /// kaspa-pq PQ-only: when set, mempool standardness requires every output AND every spent
    /// input UTXO to be the ML-DSA-87 P2PKH class (the only kaspa-pq standard class), matching
    /// the PQ-only consensus rule instead of the upstream legacy-permissive relay policy. `true`
    /// for production (this fork enforces PQ at genesis on every net); the legacy-fixture unit
    /// tests opt out (`pq_only = false`) to keep exercising the upstream class behavior.
    pub pq_only: bool,

    /// kaspa-pq DNS-finality: local mempool/mining policy for `StakeAttestationShard` txs (expiry,
    /// dedup, recent-epoch template preference). Sourced from the chain's `DnsParams`; defaults to
    /// disabled so behavior is byte-identical to upstream unless explicitly wired (see the daemon).
    pub attestation_policy: AttestationMempoolPolicy,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        maximum_transaction_count: usize,
        mempool_size_limit: usize,
        maximum_build_block_template_attempts: u64,
        transaction_expire_interval_daa_score: u64,
        transaction_expire_scan_interval_daa_score: u64,
        transaction_expire_scan_interval_milliseconds: u64,
        accepted_transaction_expire_interval_daa_score: u64,
        accepted_transaction_expire_scan_interval_daa_score: u64,
        accepted_transaction_expire_scan_interval_milliseconds: u64,
        orphan_expire_interval_daa_score: u64,
        orphan_expire_scan_interval_daa_score: u64,
        maximum_orphan_transaction_mass: u64,
        maximum_orphan_transaction_count: u64,
        accept_non_standard: bool,
        maximum_mass_per_block: u64,
        minimum_relay_transaction_fee: u64,
        minimum_standard_transaction_version: u16,
        maximum_standard_transaction_version: u16,
        network_blocks_per_second: u64,
    ) -> Self {
        Self {
            maximum_transaction_count,
            mempool_size_limit,
            maximum_build_block_template_attempts,
            transaction_expire_interval_daa_score,
            transaction_expire_scan_interval_daa_score,
            transaction_expire_scan_interval_milliseconds,
            accepted_transaction_expire_interval_daa_score,
            accepted_transaction_expire_scan_interval_daa_score,
            accepted_transaction_expire_scan_interval_milliseconds,
            orphan_expire_interval_daa_score,
            orphan_expire_scan_interval_daa_score,
            maximum_orphan_transaction_mass,
            maximum_orphan_transaction_count,
            accept_non_standard,
            maximum_mass_per_block,
            minimum_relay_transaction_fee,
            minimum_standard_transaction_version,
            maximum_standard_transaction_version,
            network_blocks_per_second,
            // kaspa-pq PQ-only relay is OFF in the base config so the (non-ML-DSA) mempool unit
            // tests and the `MiningManager::new` test path keep the upstream class behavior; the
            // production node turns it on explicitly (see `MiningManager::new`'s `pq_only` arg /
            // the daemon wiring).
            pq_only: false,
            // kaspa-pq DNS-finality: disabled by default (overlay off); the daemon overrides this
            // with values derived from the chain's `DnsParams` when present.
            attestation_policy: AttestationMempoolPolicy::disabled(),
        }
    }

    /// Build a default config.
    /// The arguments should be obtained from the current consensus [`kaspa_consensus_core::config::params::Params`] instance.
    pub fn build_default(target_milliseconds_per_block: u64, relay_non_std_transactions: bool, max_block_mass: u64) -> Self {
        Self {
            maximum_transaction_count: DEFAULT_MAXIMUM_TRANSACTION_COUNT,
            mempool_size_limit: DEFAULT_MEMPOOL_SIZE_LIMIT,
            maximum_build_block_template_attempts: DEFAULT_MAXIMUM_BUILD_BLOCK_TEMPLATE_ATTEMPTS,
            transaction_expire_interval_daa_score: DEFAULT_TRANSACTION_EXPIRE_INTERVAL_SECONDS * 1000 / target_milliseconds_per_block,
            transaction_expire_scan_interval_daa_score: DEFAULT_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000
                / target_milliseconds_per_block,
            transaction_expire_scan_interval_milliseconds: DEFAULT_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000,
            accepted_transaction_expire_interval_daa_score: DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_INTERVAL_SECONDS * 1000
                / target_milliseconds_per_block,
            accepted_transaction_expire_scan_interval_daa_score: DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000
                / target_milliseconds_per_block,
            accepted_transaction_expire_scan_interval_milliseconds: DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000,
            orphan_expire_interval_daa_score: DEFAULT_ORPHAN_EXPIRE_INTERVAL_SECONDS * 1000 / target_milliseconds_per_block,
            orphan_expire_scan_interval_daa_score: DEFAULT_ORPHAN_EXPIRE_SCAN_INTERVAL_SECONDS * 1000 / target_milliseconds_per_block,
            maximum_orphan_transaction_mass: DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_MASS,
            maximum_orphan_transaction_count: DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_COUNT,
            accept_non_standard: relay_non_std_transactions,
            maximum_mass_per_block: max_block_mass,
            minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
            minimum_standard_transaction_version: DEFAULT_MINIMUM_STANDARD_TRANSACTION_VERSION,
            maximum_standard_transaction_version: DEFAULT_MAXIMUM_STANDARD_TRANSACTION_VERSION,
            // **Never zero, whatever the cadence.** `1000 / target_milliseconds_per_block` is
            // integer division: at PALW's frozen 120 s it is `1000 / 120_000 = 0`, and every
            // consumer divides by it. The feerate estimator then computes
            // `avg_mass / (mass_per_block × 0) = +inf` and asserts, taking the whole node process
            // with it — measured on testnet-12, twice, the second time on the very transaction
            // that carries a receipt quorum.
            //
            // A sub-1-BPS network is not "zero blocks per second"; it is a fraction. `u64` cannot
            // hold that, so the floor is 1 and the exact cadence rides
            // `target_milliseconds_per_block` beside it — which every rate calculation that needs
            // sub-1 precision should read instead. Rounding the RATE up understates the interval
            // between transactions, which is the safe direction: it never inflates a fee estimate.
            network_blocks_per_second: (1000 / target_milliseconds_per_block).max(1),
            // kaspa-pq PQ-only relay is OFF in the base config so the (non-ML-DSA) mempool unit
            // tests and the `MiningManager::new` test path keep the upstream class behavior; the
            // production node turns it on explicitly (see `MiningManager::new`'s `pq_only` arg /
            // the daemon wiring).
            pq_only: false,
            // kaspa-pq DNS-finality: disabled by default (overlay off); the daemon overrides this
            // with values derived from the chain's `DnsParams` when present.
            attestation_policy: AttestationMempoolPolicy::disabled(),
        }
    }

    pub fn apply_ram_scale(mut self, ram_scale: f64) -> Self {
        // Allow only scaling down
        self.maximum_transaction_count = (self.maximum_transaction_count as f64 * ram_scale.min(1.0)) as usize;
        self.mempool_size_limit = (self.mempool_size_limit as f64 * ram_scale.min(1.0)) as usize;
        self
    }

    /// Returns the minimum standard fee/mass ratio currently required by the mempool
    pub(crate) fn minimum_feerate(&self) -> f64 {
        // The parameter minimum_relay_transaction_fee is in sompi/kg units so divide by 1000 to get sompi/gram
        self.minimum_relay_transaction_fee as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A 120-second network must not report zero blocks per second** (testnet-12, 2026-08-22).
    ///
    /// `1000 / target_milliseconds_per_block` is integer division, and PALW's frozen cadence is
    /// 120,000 ms — so this was 0, and the feerate estimator's `avg_mass / (mass_per_block × bps)`
    /// became `+inf`. The node asserted and the whole process exited, on the transaction carrying a
    /// receipt quorum: the first transaction a PALW network ever really needs. The estimator's own
    /// `< 1.0` bound had already been widened for the same launch; that fixed the value being too
    /// large, and this fixes it being infinite — two faces of "bps >= 1 was assumed everywhere".
    #[test]
    fn the_block_rate_is_never_zero_however_slow_the_network() {
        for ms in [120_000u64, 60_000, 10_000, 1_000, 100] {
            let cfg = Config::build_default(ms, false, 500_000);
            assert!(cfg.network_blocks_per_second >= 1, "{ms} ms/block reported {} bps", cfg.network_blocks_per_second);
            // And the quantity the estimator divides by is finite and positive.
            let interval = 1_000f64 / (cfg.maximum_mass_per_block as f64 * cfg.network_blocks_per_second as f64);
            assert!(interval.is_finite() && interval > 0.0, "{ms} ms/block yields interval {interval}");
        }
    }
}
