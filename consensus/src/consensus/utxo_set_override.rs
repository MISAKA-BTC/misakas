use std::sync::Arc;

use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    config::{Config, premine::genesis_premine_utxos_for},
    header::Header,
    muhash::MuHashExtensions,
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_muhash::MuHash;

use crate::consensus::Consensus;

/// The genesis UTXO set imported at consensus initialization: the canonical kaspa-pq (misaka)
/// premine — **exactly the 10B cap on every network** (see
/// `kaspa_consensus_core::config::premine`): one main-wallet UTXO, from which testnet-11
/// additionally carves its genesis-bond collateral, per-bond fee floats and the community
/// allocation — plus, when the `devnet-prealloc` feature is enabled, any CLI-preallocated
/// UTXOs from `config.initial_utxo_set`. Each UTXO is a single-key ML-DSA-87 P2PKH; the main
/// wallet is the operator custody address on mainnet, the operator's public PALW address on
/// testnet-11, and the Claude-managed test key elsewhere.
fn genesis_initial_utxo_set(config: &Config) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    // `mut` is only exercised under `devnet-prealloc` (the extend below).
    #[cfg_attr(not(feature = "devnet-prealloc"), allow(unused_mut))]
    // Keyed by the full NetworkId: testnet-11 carries the carved-out extras; every other
    // network carries the main wallet alone.
    let mut set: Vec<(TransactionOutpoint, UtxoEntry)> = genesis_premine_utxos_for(config.params.net).into_iter().collect();
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
mod repin {
    use super::*;
    use kaspa_consensus_core::config::params::palw_rc_shipped_params;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};

    /// **Print the genesis constants the current premine set implies.** Run when the premine
    /// changes: `cargo test -p kaspa-consensus --lib repin::print -- --ignored --nocapture`.
    /// The M-07 guard refuses to boot on a mismatch, deliberately — this is how the pin is
    /// recomputed rather than guessed.
    #[test]
    #[ignore]
    fn print_repinned_rc_genesis() {
        let mut params = palw_rc_shipped_params();
        params.net = NetworkId::with_suffix(NetworkType::Testnet, 11);
        let mut ms = MuHash::new();
        for (outpoint, entry) in kaspa_consensus_core::config::premine::genesis_premine_utxos_for(params.net) {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        params.genesis.utxo_commitment = commitment;
        let header: kaspa_consensus_core::header::Header = (&params.genesis).into();
        let rust = |b: &[u8]| {
            let mut out = String::from("[\n");
            for chunk in b.chunks(21) {
                out.push_str("        ");
                for x in chunk {
                    out.push_str(&format!("0x{x:02x}, "));
                }
                out.push('\n');
            }
            out.push_str("    ]");
            out
        };
        println!("REPIN utxo_commitment: Hash64::from_bytes({}),", rust(commitment.as_byte_slice()));
        println!("REPIN hash: Hash64::from_bytes({}),", rust(header.hash.as_byte_slice()));
    }

    /// **Print the genesis constants a filled mainnet card implies** (mainnet audit 2026-09-06,
    /// M-7). Run in the same commit that fills `PALW_MAINNET_GENESIS_BONDS` and the artifact roots:
    /// `cargo test -p kaspa-consensus --lib -- repin::print_repinned_mainnet -- --ignored
    /// --nocapture`.
    ///
    /// TWO things move a carded mainnet's genesis and the ceremony has to re-pin both. The premine
    /// moves because the seats' collateral and fee floats are carved from the main wallet; `bits`
    /// moves because a V2 network is minted at its ambient maximum and `MAINNET_GENESIS` carries
    /// the hash lineage's value, 256x harder (`palw_v2_params_on_base`, and the gate at
    /// `validate_palw_v2`). Miss the second and `set_genesis_utxo_commitment_from_config`'s M-07
    /// assert refuses to boot with a message about the premine, which is the wrong diagnosis.
    #[test]
    #[ignore]
    fn print_repinned_mainnet_card_genesis() {
        use kaspa_consensus_core::config::params::mainnet_shipped_params;
        let mut params = mainnet_shipped_params();
        let mut ms = MuHash::new();
        for (outpoint, entry) in kaspa_consensus_core::config::premine::genesis_premine_utxos_for(params.net) {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        params.genesis.utxo_commitment = commitment;
        let header: kaspa_consensus_core::header::Header = (&params.genesis).into();
        println!("REPIN GENESIS.bits: {:#010x},", params.genesis.bits);
        println!("REPIN GENESIS.utxo_commitment bytes: {:?}", commitment.as_byte_slice());
        println!("REPIN GENESIS.hash bytes: {:?}", header.hash.as_byte_slice());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        config::{Config, params::SIMNET_PARAMS, premine::misaka_premine_utxos},
        constants::SOMPI_PER_KASPA,
        muhash::MuHashExtensions,
        network::NetworkType,
    };

    #[test]
    fn premine_is_the_expected_split() {
        // Re-genesis 2026-08-30: one main-wallet UTXO of exactly the 10B cap, on EVERY network
        // (the vault block is gone; testnet-11's extras are carved from the main wallet).
        // `config::premine::tests::every_network_genesis_mints_exactly_the_10b_cap` asserts the
        // cap across all networks; this fixture is simnet.
        let utxos = misaka_premine_utxos(NetworkType::Simnet);
        assert_eq!(utxos.len(), 1, "the premine is one main-wallet UTXO");
        let total: u64 = utxos.values().map(|e| e.amount).sum();
        assert_eq!(total, 10_000_000_000 * SOMPI_PER_KASPA, "the 10B cap");
        for entry in utxos.values() {
            assert!(!entry.is_coinbase, "premine must be non-coinbase (spendable from block 0)");
            assert_eq!(entry.block_daa_score, 0);
            // kaspa-pq ML-DSA-87 P2PKH template (ADR-0019 §8): OP_DUP OP_BLAKE2B_512
            // OP_DATA64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87 = 69 bytes.
            assert_eq!(entry.script_public_key.script().len(), 69);
        }
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
        use kaspa_consensus_core::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params,
        };
        // **The preset a suffix actually routes to, not the const that shares its name.**
        //
        // `TESTNET11_PARAMS` is the legacy algo-4 lane; `From<NetworkId>` has not returned it since
        // the PALW-RC network moved onto suffix 11, so its pinned genesis is a genesis nothing
        // builds. Checking it against a premine keyed to suffix 11 compares two different networks
        // — the const's genesis was computed without the RC's per-bond fee floats, which the
        // premine for that suffix now carries.
        for params in [MAINNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params(), DEVNET_PARAMS, SIMNET_PARAMS] {
            let mut config = Config::new(params);
            // The assert inside panics if the pinned constants do not match the premine-derived
            // ones — for testnet-11 that set includes the 347M community allocation.
            set_genesis_utxo_commitment_from_config(&mut config);
        }
    }
}
