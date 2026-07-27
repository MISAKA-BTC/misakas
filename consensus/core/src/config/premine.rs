//! kaspa-pq (misaka) genesis premine — 10B single-UTXO grant (re-genesis 2026-07-20).
//!
//! One "main" UTXO of **10B MSK** per network, baked into genesis. This is the genesis
//! portion of the **~26.013224875B** final supply (the other ~16.013224875B is mined
//! over 30 years at 1.4%/yr decay; see the emission table in
//! `consensus/src/processes/coinbase.rs`). The premine was reduced
//! 13B → 10B in this re-genesis (the mined half is the 30-year 16.013224875B schedule),
//! and the former 40-vault split was collapsed into a single grant per network — the
//! operator now holds the whole genesis allocation at one custody address and performs
//! any vault split as ordinary on-chain transactions after launch.
//!
//! The UTXO locks to the standard single-key ML-DSA-87 P2PKH `scriptPubKey`
//! `OP_DUP OP_BLAKE2B_512 OP_DATA_64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`
//! (built by [`crate::dns_finality::p2pkh_mldsa87_spk`]), where the 64-byte payload is
//! the keyed BLAKE2b-512 address payload decoded from the recipient address. The
//! address is stored as text (not an opaque hash) so the premine is auditable.
//!
//! ## Custody — per-network main wallet (audit H-01)
//!
//! * **Mainnet** grants the 10B to the operator custody address
//!   ([`MAINNET_MAIN_ADDRESS`]) — an ML-DSA-87 key held offline by the operator.
//! * **Testnet / devnet / simnet** grant the 10B to the operator's test-net address
//!   ([`TESTNET_MAIN_ADDRESS`]), which holds value-less test coins and is used to fund /
//!   stand up validators during E2E validation. The two payloads MUST differ
//!   (`mainnet_premine_is_spendable_custody`), so a test key can never receive mainnet
//!   value.
//!
//! Multisig / P2SH is out of launch scope (ADR-0019 §8/§6.5).

use crate::{
    constants::SOMPI_PER_KASPA,
    network::NetworkType,
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_addresses::{Address, Version};
use kaspa_hashes::Hash64;

/// Main-wallet premine amount: 10B KAS — the entire genesis allocation.
pub const MAIN_PREMINE_SOMPI: u64 = 10_000_000_000 * SOMPI_PER_KASPA;
/// Number of premine UTXOs: exactly one (the 10B main grant).
pub const PREMINE_UTXO_COUNT: usize = 1;
/// Total genesis premine = **10B KAS**.
pub const MISAKA_PREMINE_SOMPI: u64 = MAIN_PREMINE_SOMPI;

// ---------------------------------------------------------------------------------------------
// DERIVATION NOTE (issue #14) — read before regenerating either address below.
//
// Both custody addresses were minted under the derivation as it stood BEFORE the "audit L"
// domain-separation fix, i.e. `kaspa_wallet_keys::kaspa_pq::derive_keygen_seed_LEGACY`, not the
// length-prefixed `derive_keygen_seed` shipping today. Re-deriving the custody wallet from its
// BIP39 mnemonic with current code yields a DIFFERENT address and the premine looks lost.
// Confirmed empirically for testnet: the mnemonic reproduces `TESTNET_MAIN_ADDRESS` at
// ("testnet-10", 0, 0, 0) under the legacy scheme and under no coordinate of the current one.
//
// These constants CANNOT be corrected in place: `misaka_premine_utxos` feeds the genesis
// `utxo_commitment`, so each address is bound into its network's genesis hash. Changing one is a
// re-genesis, and testnet-10 is live. Operator tooling must use the legacy derivation to spend
// the premine; every NEW key must use the current one.
// ---------------------------------------------------------------------------------------------

/// Mainnet main-wallet (10B) custody address (operator-held ML-DSA-87 key).
///
/// See the DERIVATION NOTE above. **Launch gate:** confirm which derivation reaches this address
/// before mainnet launch — if it was minted the same way as the testnet one (very likely, same
/// ceremony), rebuilding the operator wallet from current source will not reach the mainnet
/// premine either.
const MAINNET_MAIN_ADDRESS: &str =
    "misaka:qfckqxaaxfks4mn783zpg0cdw9v8h09rx253c0vdf5722nj6zhsyv83aqpp99hr4kfnl9fetrhegtga9jgqrzpgnvmkndg6pwmd2m2xddvq0asl7";

/// Testnet/devnet/simnet main-wallet (10B) address — operator-held, value-less test coins.
///
/// See the DERIVATION NOTE above: reachable only via the legacy derivation, at
/// ("testnet-10", account 0, change 0, index 0).
const TESTNET_MAIN_ADDRESS: &str =
    "misakatest:qflkp962vgaqckl8zvf0w352hr7zkf3csrqmr4u9wf7nccdqy87x86fee0dk7tz2muc3kzrmmfy0f37cm8a2apjs0cedl5levr3l9nyv4phyke8k";

/// audit H-01: the mainnet premine ceremony is **COMPLETE** — the custody address
/// above replaces the former all-zero unspendable placeholder, so mainnet is no longer
/// locked. Guarded by `mainnet_premine_is_spendable_custody`.
pub const MAINNET_PREMINE_CEREMONY_PENDING: bool = false;

/// Deterministic sentinel txid for the premine UTXO: ASCII "misaka-premine" (14
/// bytes) zero-padded to the 64-byte `Hash64` width. The UTXO sits at index 0 on this
/// txid; fixed because it feeds the genesis `utxo_commitment`.
#[rustfmt::skip]
const MISAKA_PREMINE_TXID: [u8; 64] = [
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x72, 0x65, 0x6d, 0x69, 0x6e, 0x65, // "misaka-premine"
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Decode a premine recipient address to its 64-byte ML-DSA-87 owner payload. Panics
/// on a malformed address or wrong version — a startup guard analogous to the H-01
/// ceremony guard: a typo in a premine address must fail loudly, never silently lock
/// funds to the wrong script.
fn owner_payload(addr: &str) -> [u8; 64] {
    let a = Address::try_from(addr).unwrap_or_else(|e| panic!("premine address {addr} is invalid: {e:?}"));
    assert_eq!(a.version, Version::PubKeyHashMlDsa87, "premine address {addr} must be single-key ML-DSA-87 P2PKH");
    let p = a.payload.as_slice();
    assert_eq!(p.len(), 64, "premine address {addr} payload must be 64 bytes");
    let mut out = [0u8; 64];
    out.copy_from_slice(p);
    out
}

/// The 10B main-wallet address for `network_type` (audit H-01): mainnet uses the
/// operator custody address; every test network uses the operator's test-net address.
fn main_address(network_type: NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => MAINNET_MAIN_ADDRESS,
        NetworkType::Testnet | NetworkType::Devnet | NetworkType::Simnet => TESTNET_MAIN_ADDRESS,
    }
}

/// The canonical kaspa-pq genesis premine UTXO set for `network_type`: a single 10B
/// main UTXO at index 0, single-key ML-DSA-87 P2PKH and spendable from block 0
/// (`is_coinbase: false`, no maturity delay). The recipient is per-network (see
/// [`main_address`]).
pub fn misaka_premine_utxos(network_type: NetworkType) -> UtxoCollection {
    let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
    let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main_address(network_type)));
    let outpoint = TransactionOutpoint { transaction_id: txid, index: 0 };
    UtxoCollection::from_iter([(
        outpoint,
        UtxoEntry { amount: MAIN_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false },
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::muhash::MuHashExtensions;
    use kaspa_muhash::MuHash;

    /// Prints the per-network genesis `utxo_commitment`s to hardcode in `genesis.rs`.
    /// Run:
    /// `cargo test -p kaspa-consensus-core --lib config::premine::tests::print_premine_commitment -- --nocapture`
    #[test]
    fn print_premine_commitment() {
        for net in [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet] {
            let mut ms = MuHash::new();
            for (outpoint, entry) in misaka_premine_utxos(net) {
                ms.add_utxo(&outpoint, &entry);
            }
            let commitment = ms.finalize();
            let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
            println!("{net:?}_PREMINE_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
        }
    }

    /// The premine is exactly 1 UTXO of 10B KAS, a 69-byte ML-DSA-87 P2PKH spendable
    /// from block 0.
    #[test]
    fn premine_is_the_10b_grant() {
        for net in [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet] {
            let utxos = misaka_premine_utxos(net);
            assert_eq!(utxos.len(), PREMINE_UTXO_COUNT, "premine is a single main UTXO");
            let total: u64 = utxos.values().map(|e| e.amount).sum();
            assert_eq!(total, MISAKA_PREMINE_SOMPI, "premine total");
            assert_eq!(total, 10_000_000_000 * SOMPI_PER_KASPA, "10B KAS");
            for entry in utxos.values() {
                assert!(!entry.is_coinbase, "premine must be non-coinbase (spendable from block 0)");
                assert_eq!(entry.block_daa_score, 0);
                assert_eq!(entry.script_public_key.script().len(), 69, "ML-DSA-87 P2PKH = 69 bytes");
            }
        }
    }

    /// The premine UTXO pays exactly the network's configured custody address — i.e.
    /// the script is the P2PKH of that address's payload, not some other key.
    #[test]
    fn premine_pays_the_configured_custody_address() {
        for net in [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet] {
            let expected = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main_address(net)));
            let utxos = misaka_premine_utxos(net);
            let entry = utxos.values().next().expect("one premine UTXO");
            assert_eq!(entry.script_public_key, expected, "{net:?}: premine must pay the configured custody address");
        }
    }

    /// audit H-01: the mainnet premine must be spendable custody (not the all-zero
    /// placeholder) and distinct from the test-network key, so mainnet value can never
    /// be locked to an unspendable key or to a key that also holds test coins.
    #[test]
    fn mainnet_premine_is_spendable_custody() {
        let mainnet_main = owner_payload(MAINNET_MAIN_ADDRESS);
        assert_ne!(mainnet_main, [0u8; 64], "mainnet main wallet must not be the all-zero placeholder");
        assert_ne!(mainnet_main, owner_payload(TESTNET_MAIN_ADDRESS), "mainnet main must differ from the test key");
        assert!(!MAINNET_PREMINE_CEREMONY_PENDING, "ceremony is complete (custody address installed)");
    }

    /// The custody addresses carry the network prefix they are used on, so a mainnet
    /// grant can never be pasted from a `misakatest:` address (or vice versa).
    #[test]
    fn custody_addresses_carry_the_right_prefix() {
        assert!(MAINNET_MAIN_ADDRESS.starts_with("misaka:"), "mainnet custody must be a `misaka:` address");
        assert!(TESTNET_MAIN_ADDRESS.starts_with("misakatest:"), "test custody must be a `misakatest:` address");
    }
}
