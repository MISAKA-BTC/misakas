//! kaspa-pq (misaka) genesis premine.
//!
//! A single 15B KAS UTXO locked to a **single-key ML-DSA-87 P2PKH**, baked into
//! genesis on every network (same script everywhere). This is the genesis half
//! of the 30B final supply (the other 15B is mined over 20 years; see the
//! emission table in `consensus/src/processes/coinbase.rs`).
//!
//! The lock is the standard ML-DSA P2PKH `scriptPubKey`
//! `OP_DUP OP_BLAKE2B_512 OP_DATA_64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`
//! (built by `crate::dns_finality::p2pkh_mldsa87_spk`), where the 64-byte payload
//! is the BLAKE2b-512 of the single premine public key. The devnet signing key is
//! in the repo-root `misaka-devnet-premine-key.json` and the devnet address is
//! `misakadev:qgtad66msrm6qvkvum8m3450zlkegqhrxfwaygwa5hfll4ys2xtj8xuj8fr0elvvk80k79nxplrm05ldr40tqkjdpgq66js2aa6v4uv8g73c5q4p`.
//! Multisig / P2SH is out of launch scope (ADR-0019 §8/§6.5). Regenerate the key
//! via the `gen_misaka_devnet_premine_key` test in this module.

use crate::{
    constants::SOMPI_PER_KASPA,
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::Hash64;

/// Premine amount: 15B KAS.
pub const MISAKA_PREMINE_SOMPI: u64 = 15_000_000_000 * SOMPI_PER_KASPA;

/// BLAKE2b-512 of the single-key ML-DSA-87 premine public key; the 15B genesis
/// UTXO is locked to its P2PKH. Regenerate via `gen_misaka_devnet_premine_key`.
/// ADR-0019.
#[rustfmt::skip]
const MISAKA_PREMINE_OWNER_PAYLOAD: [u8; 64] = [
    0x17, 0xd6, 0xeb, 0x5b, 0x80, 0xf7, 0xa0, 0x32, 0xcc, 0xe6, 0xcf, 0xb8, 0xd6, 0x8f, 0x17, 0xed,
    0x94, 0x02, 0xe3, 0x32, 0x5d, 0xd2, 0x21, 0xdd, 0xa5, 0xd3, 0xff, 0xd4, 0x90, 0x51, 0x97, 0x23,
    0x9b, 0x92, 0x3a, 0x46, 0xfc, 0xfd, 0x8c, 0xb1, 0xdf, 0x6f, 0x16, 0x66, 0x0f, 0xc7, 0xb7, 0xd3,
    0xed, 0x1d, 0x5e, 0xb0, 0x5a, 0x4d, 0x0a, 0x01, 0xad, 0x4a, 0x0a, 0xef, 0x74, 0xca, 0xf1, 0x87,
];

/// Deterministic sentinel txid for the single premine UTXO: ASCII "misaka-premine"
/// (14 bytes) zero-padded to the 64-byte `Hash64` width. Fixed because it feeds
/// the genesis `utxo_commitment`.
#[rustfmt::skip]
const MISAKA_PREMINE_TXID: [u8; 64] = [
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x72, 0x65, 0x6d, 0x69, 0x6e, 0x65, // "misaka-premine"
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// The canonical kaspa-pq genesis premine UTXO set: one 15B KAS single-key
/// ML-DSA-87 P2PKH UTXO, spendable from block 0 (`is_coinbase: false`, no
/// maturity delay).
pub fn misaka_premine_utxos() -> UtxoCollection {
    let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&MISAKA_PREMINE_OWNER_PAYLOAD);
    let outpoint = TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index: 0 };
    let entry = UtxoEntry { amount: MISAKA_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false };
    UtxoCollection::from_iter([(outpoint, entry)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::muhash::MuHashExtensions;
    use kaspa_muhash::MuHash;

    /// Prints the genesis `utxo_commitment` to hardcode in `genesis.rs`. Run:
    /// `cargo test -p kaspa-consensus-core --lib config::premine::tests::print_premine_commitment -- --nocapture`
    #[test]
    fn print_premine_commitment() {
        let mut ms = MuHash::new();
        for (outpoint, entry) in misaka_premine_utxos() {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        println!("PREMINE_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
    }

    /// Deterministically derives the single-key ML-DSA-87 premine keypair and its
    /// 64-byte BLAKE2b-512 owner payload (the value baked into
    /// `MISAKA_PREMINE_OWNER_PAYLOAD`), prints the payload as a pasteable Rust
    /// array literal + the devnet address, and writes `misaka-devnet-premine-key.json`
    /// at the repo root. ADR-0019 §8/§6.5 (single-key P2PKH, no multisig/P2SH).
    /// Run (ignored by default):
    /// `cargo test -p kaspa-consensus-core --lib config::premine::tests::gen_misaka_devnet_premine_key -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gen_misaka_devnet_premine_key() {
        use blake2b_simd::Params;
        use kaspa_addresses::{Address, Prefix, Version};
        use kaspa_hashes::blake2b_512;
        use libcrux_ml_dsa::ml_dsa_87;

        // Documented deterministic devnet seed: BLAKE2b-256("misaka-devnet-premine-key").
        let seed_hash = Params::new().hash_length(32).hash(b"misaka-devnet-premine-key");
        let mut seed = [0u8; 32];
        seed.copy_from_slice(seed_hash.as_bytes());

        // One ML-DSA-87 keypair from that seed.
        let key_pair = ml_dsa_87::generate_key_pair(seed);
        let pubkey = key_pair.verification_key.as_ref();

        // Owner payload = BLAKE2b-512(public key) — the 64-byte address payload.
        let payload: [u8; 64] = blake2b_512(pubkey).as_bytes();

        // Standard ML-DSA P2PKH scriptPubKey must be exactly 69 bytes.
        let spk = crate::dns_finality::p2pkh_mldsa87_spk(&payload);
        assert_eq!(spk.script().len(), 69, "ML-DSA P2PKH scriptPubKey must be 69 bytes");

        let payload_rust = payload.iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        let payload_hex = payload.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let seed_hex = seed.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let pubkey_hex = pubkey.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let devnet_address = Address::new(Prefix::Devnet, Version::PubKeyHashMlDsa87, &payload).to_string();

        println!("MISAKA_PREMINE_OWNER_PAYLOAD: [{payload_rust}]");
        println!("DEVNET_ADDRESS: {devnet_address}");
        println!("SEED_HEX: {seed_hex}");
        println!("PUBKEY_HEX: {pubkey_hex}");

        let json = format!(
            "{{\n  \"scheme\": \"ml-dsa-87 single-key P2PKH premine\",\n  \"devnet_address\": \"{devnet_address}\",\n  \"owner_payload_blake2b512\": \"{payload_hex}\",\n  \"seed_hex\": \"{seed_hex}\",\n  \"pubkey_hex\": \"{pubkey_hex}\"\n}}\n"
        );
        // Repo root = three levels up from consensus/core/src.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../misaka-devnet-premine-key.json");
        std::fs::write(path, json).expect("write misaka-devnet-premine-key.json");
        println!("WROTE: {path}");
    }
}
