use std::sync::Arc;

use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    config::{Config, premine::misaka_premine_utxos},
    header::Header,
    muhash::MuHashExtensions,
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_muhash::MuHash;

use crate::consensus::Consensus;

/// The genesis UTXO set imported at consensus initialization: the canonical
/// kaspa-pq (misaka) premine — a single 15B KAS UTXO locked to a single-key
/// ML-DSA-87 P2PKH whose owner payload is network-dependent (see
/// `kaspa_consensus_core::config::premine`: an unspendable ceremony placeholder on
/// mainnet, the public test key on testnet/devnet/simnet) — plus, when the
/// `devnet-prealloc` feature is enabled, any CLI-preallocated UTXOs from
/// `config.initial_utxo_set`.
fn genesis_initial_utxo_set(config: &Config) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    // `mut` is only exercised under `devnet-prealloc` (the extend below).
    #[cfg_attr(not(feature = "devnet-prealloc"), allow(unused_mut))]
    let mut set: Vec<(TransactionOutpoint, UtxoEntry)> =
        misaka_premine_utxos(config.params.net.network_type).into_iter().collect();
    #[cfg(feature = "devnet-prealloc")]
    set.extend(config.initial_utxo_set.iter().map(|(op, entry)| (*op, entry.clone())));
    #[cfg(not(feature = "devnet-prealloc"))]
    let _ = config;
    set
}

/// Derives the genesis `utxo_commitment` (and the resulting genesis block hash)
/// from the baked-in premine UTXO set. Called unconditionally for every network,
/// so all nodes agree on the premine-aware genesis identity.
pub fn set_genesis_utxo_commitment_from_config(config: &mut Config) {
    // audit M-07: the hardcoded `GENESIS.hash`/`utxo_commitment` MUST equal the premine-derived
    // values, so an operator can never silently run a divergent genesis (e.g. a premine payload
    // edited — or a ceremony payload installed — without re-pinning the constants). We recompute and
    // then assert equality below.
    let hardcoded_commitment = config.params.genesis.utxo_commitment;
    let hardcoded_hash = config.params.genesis.hash;

    let mut genesis_multiset = MuHash::new();
    for (outpoint, entry) in genesis_initial_utxo_set(config) {
        genesis_multiset.add_utxo(&outpoint, &entry);
    }

    config.params.genesis.utxo_commitment = genesis_multiset.finalize();
    let genesis_header: Header = (&config.params.genesis).into();
    config.params.genesis.hash = genesis_header.hash;

    // The canonical premine MUST round-trip to the pinned constants. Skipped under
    // `devnet-prealloc`, where CLI-injected UTXOs legitimately change the commitment.
    #[cfg(not(feature = "devnet-prealloc"))]
    {
        assert_eq!(
            config.params.genesis.utxo_commitment, hardcoded_commitment,
            "genesis utxo_commitment mismatch (audit M-07): the pinned GENESIS.utxo_commitment does not match the premine UTXO set — re-pin it after any premine change via the config::premine ceremony tool"
        );
        assert_eq!(
            config.params.genesis.hash, hardcoded_hash,
            "genesis hash mismatch (audit M-07): the pinned GENESIS.hash does not match the premine-derived hash — re-pin GENESIS.hash + utxo_commitment after any premine change"
        );
    }
    #[cfg(feature = "devnet-prealloc")]
    {
        let _ = (hardcoded_commitment, hardcoded_hash);
    }
}

/// Imports the premine UTXO set into a freshly created consensus. The imported
/// multiset is validated against the genesis `utxo_commitment` set above.
pub fn set_initial_utxo_set(config: &Config, consensus: Arc<Consensus>, genesis_hash: BlockHash) {
    let utxo_set = genesis_initial_utxo_set(config);
    let mut genesis_multiset = MuHash::new();
    consensus.append_imported_pruning_point_utxos(&utxo_set, &mut genesis_multiset);
    consensus.import_pruning_point_utxo_set(genesis_hash, genesis_multiset).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        config::{Config, params::SIMNET_PARAMS, premine::MISAKA_PREMINE_SOMPI},
        constants::SOMPI_PER_KASPA,
        muhash::MuHashExtensions,
        network::NetworkType,
    };

    #[test]
    fn premine_is_a_single_15b_utxo() {
        let utxos = misaka_premine_utxos(NetworkType::Simnet);
        assert_eq!(utxos.len(), 1, "premine is a single UTXO");
        let entry = utxos.values().next().unwrap();
        assert_eq!(entry.amount, MISAKA_PREMINE_SOMPI);
        assert_eq!(entry.amount, 15_000_000_000 * SOMPI_PER_KASPA, "15B KAS");
        assert!(!entry.is_coinbase, "premine must be non-coinbase (spendable from block 0)");
        assert_eq!(entry.block_daa_score, 0);
        // kaspa-pq ML-DSA-87 P2PKH template (ADR-0019 §8): OP_DUP OP_BLAKE2B_512
        // OP_DATA64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87 = 69 bytes.
        assert_eq!(entry.script_public_key.script().len(), 69);
    }

    #[test]
    fn static_genesis_commits_to_premine_and_recompute_is_idempotent() {
        // The expected commitment is the MuHash over the premine UTXO set. The
        // test config is SIMNET (a test network), so the premine uses the public
        // test owner payload — unchanged by the mainnet custody split (audit H-01).
        let mut ms = MuHash::new();
        for (outpoint, entry) in misaka_premine_utxos(NetworkType::Simnet) {
            ms.add_utxo(&outpoint, &entry);
        }
        let expected_commitment = ms.finalize();

        // The static genesis (params const) already commits to the premine, so
        // the runtime genesis identity equals the hardcoded `*_GENESIS.hash`.
        let config = Config::new(SIMNET_PARAMS);
        assert_eq!(config.params.genesis.utxo_commitment, expected_commitment, "static genesis must commit to the premine");

        // Re-deriving the commitment must be a no-op for the canonical premine
        // (only `devnet-prealloc` additions would change it).
        let mut recomputed = config.clone();
        let static_hash = recomputed.params.genesis.hash;
        set_genesis_utxo_commitment_from_config(&mut recomputed);
        assert_eq!(recomputed.params.genesis.utxo_commitment, expected_commitment);
        assert_eq!(recomputed.params.genesis.hash, static_hash, "premine commitment recompute must be idempotent");
    }

    /// audit M-07: on EVERY network the hardcoded `GENESIS.hash` / `utxo_commitment` must round-trip
    /// to the premine UTXO set — `set_genesis_utxo_commitment_from_config` now asserts this, so this
    /// test fails (panics) the instant a network's pinned genesis constants drift from its premine
    /// (including the mainnet all-zero-placeholder premine). It is the static guarantee behind the
    /// runtime "can't run a divergent genesis" property.
    #[test]
    fn all_networks_genesis_constants_match_premine() {
        use kaspa_consensus_core::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};
        for params in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS] {
            let mut config = Config::new(params);
            // The assert inside panics if the pinned constants do not match the premine-derived ones.
            set_genesis_utxo_commitment_from_config(&mut config);
        }
    }
}
