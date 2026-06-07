//! kaspa-pq (misaka) genesis premine.
//!
//! A single 15B KAS UTXO locked to a **single-key ML-DSA-87 P2PKH**, baked into
//! genesis. This is the genesis half of the 30B final supply (the other 15B is
//! mined over 20 years; see the emission table in
//! `consensus/src/processes/coinbase.rs`).
//!
//! The lock is the standard ML-DSA P2PKH `scriptPubKey`
//! `OP_DUP OP_BLAKE2B_512 OP_DATA_64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`
//! (built by `crate::dns_finality::p2pkh_mldsa87_spk`), where the 64-byte payload
//! is the keyed BLAKE2b-512 (md2 §4.2, `kaspa-pq-v2/address/mldsa87`) of the
//! single premine public key.
//!
//! ## Custody — per-network owner payload (audit H-01)
//!
//! The owner payload is selected **per network** by [`premine_owner_payload`]:
//!
//! * **Test networks (testnet / devnet / simnet)** use
//!   [`PUBLIC_TEST_PREMINE_OWNER_PAYLOAD`], deterministically derived from the
//!   PUBLIC string `"misaka-devnet-premine-key"` (see
//!   [`tests::gen_public_test_premine_key`]). Its private key is therefore
//!   recoverable by **anyone** — intentional and harmless for value-less test
//!   networks (e.g. funding a faucet), which is exactly why it must never lock
//!   real value.
//! * **Mainnet** uses [`MAINNET_PREMINE_OWNER_PAYLOAD`], currently the all-zero
//!   **unspendable placeholder** (no ML-DSA-87 key hashes to an all-zero address
//!   payload). 🔴 **LAUNCH BLOCKER:** before any real-value mainnet, replace it —
//!   in an offline key-generation ceremony — with the keyed BLAKE2b-512 payload of
//!   a CSPRNG-generated ML-DSA-87 key whose private key is held in custody (HSM /
//!   PQ-multisig) and **never committed**, then re-genesis (recompute
//!   `GENESIS.utxo_commitment` + `GENESIS.hash`). The
//!   `mainnet_premine_is_not_the_public_test_key` test guarantees mainnet can
//!   never silently ship locked to the publicly-recoverable test key.
//!
//! Multisig / P2SH is out of launch scope (ADR-0019 §8/§6.5).

use crate::{
    constants::SOMPI_PER_KASPA,
    network::NetworkType,
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_hashes::Hash64;

/// Premine amount: 15B KAS.
pub const MISAKA_PREMINE_SOMPI: u64 = 15_000_000_000 * SOMPI_PER_KASPA;

/// **PUBLIC** test-network premine owner payload (testnet / devnet / simnet):
/// keyed BLAKE2b-512 (md2 §4.2 address context `kaspa-pq-v2/address/mldsa87`) of an
/// ML-DSA-87 key deterministically derived from the PUBLIC string
/// `"misaka-devnet-premine-key"`. The private key is **publicly recoverable** (see
/// `tests::gen_public_test_premine_key`); this is intentional for value-less test
/// networks and MUST NEVER lock mainnet value. ADR-0019.
#[rustfmt::skip]
const PUBLIC_TEST_PREMINE_OWNER_PAYLOAD: [u8; 64] = [
    0x08, 0x5e, 0x4d, 0x6f, 0x1d, 0xc8, 0x8b, 0x81, 0xd1, 0xd9, 0xa6, 0x97, 0x8d, 0xa8, 0x56, 0x4e,
    0xbc, 0x40, 0x33, 0xb8, 0x8b, 0x8f, 0xcc, 0x38, 0xf8, 0xb7, 0x0b, 0x54, 0xbf, 0xd4, 0xfc, 0x66,
    0x36, 0x98, 0x8a, 0x71, 0x77, 0x8e, 0x20, 0x1a, 0x4d, 0xe8, 0xb1, 0x3d, 0xd0, 0xc2, 0xc2, 0x69,
    0xc6, 0xca, 0x7c, 0x6c, 0xa0, 0x76, 0xd0, 0x10, 0xa6, 0xd8, 0x48, 0x27, 0x7c, 0xdd, 0x6e, 0xf9,
];

/// 🔴 **LAUNCH BLOCKER (audit H-01):** mainnet premine owner payload. The all-zero
/// placeholder is **unspendable** (no ML-DSA-87 verification key hashes to an
/// all-zero 64-byte address payload), so the 15B mainnet genesis UTXO is locked
/// until a ceremony-generated payload replaces it and mainnet is re-genesised.
/// Kept distinct from the public test key by construction — guarded by the
/// `mainnet_premine_is_not_the_public_test_key` test.
#[rustfmt::skip]
const MAINNET_PREMINE_OWNER_PAYLOAD: [u8; 64] = [0u8; 64];

/// `true` while [`MAINNET_PREMINE_OWNER_PAYLOAD`] is the unspendable placeholder.
/// The mainnet release runbook MUST replace the payload (offline ceremony) and
/// flip this to `false` before launch. ADR-0019 (audit H-01).
pub const MAINNET_PREMINE_CEREMONY_PENDING: bool = true;

/// Genesis premine owner payload for `network_type` (audit H-01): mainnet uses the
/// ceremony-controlled [`MAINNET_PREMINE_OWNER_PAYLOAD`]; every test network uses
/// the publicly-recoverable [`PUBLIC_TEST_PREMINE_OWNER_PAYLOAD`].
pub fn premine_owner_payload(network_type: NetworkType) -> &'static [u8; 64] {
    match network_type {
        NetworkType::Mainnet => &MAINNET_PREMINE_OWNER_PAYLOAD,
        NetworkType::Testnet | NetworkType::Devnet | NetworkType::Simnet => &PUBLIC_TEST_PREMINE_OWNER_PAYLOAD,
    }
}

/// Deterministic sentinel txid for the single premine UTXO: ASCII "misaka-premine"
/// (14 bytes) zero-padded to the 64-byte `Hash64` width. Fixed because it feeds
/// the genesis `utxo_commitment`.
#[rustfmt::skip]
const MISAKA_PREMINE_TXID: [u8; 64] = [
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x72, 0x65, 0x6d, 0x69, 0x6e, 0x65, // "misaka-premine"
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// The canonical kaspa-pq genesis premine UTXO set for `network_type`: one 15B KAS
/// single-key ML-DSA-87 P2PKH UTXO, spendable from block 0 (`is_coinbase: false`,
/// no maturity delay). The owner payload is network-dependent (see
/// [`premine_owner_payload`]): unspendable placeholder on mainnet until the
/// custody ceremony, public test key on testnet/devnet/simnet.
pub fn misaka_premine_utxos(network_type: NetworkType) -> UtxoCollection {
    let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(premine_owner_payload(network_type));
    let outpoint = TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index: 0 };
    let entry = UtxoEntry { amount: MISAKA_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false };
    UtxoCollection::from_iter([(outpoint, entry)])
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
        for net in [NetworkType::Mainnet, NetworkType::Testnet] {
            let mut ms = MuHash::new();
            for (outpoint, entry) in misaka_premine_utxos(net) {
                ms.add_utxo(&outpoint, &entry);
            }
            let commitment = ms.finalize();
            let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
            println!("{net:?}_PREMINE_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
        }
    }

    /// audit H-01: the mainnet premine MUST NOT be lockable by the publicly
    /// recoverable test key. This fails the build the instant mainnet is wired to
    /// the public test payload, so mainnet can never silently ship with a key
    /// anyone can regenerate.
    #[test]
    fn mainnet_premine_is_not_the_public_test_key() {
        assert_ne!(
            premine_owner_payload(NetworkType::Mainnet),
            premine_owner_payload(NetworkType::Testnet),
            "mainnet premine must use a distinct, ceremony-generated owner payload, never the public test key"
        );
        assert_ne!(MAINNET_PREMINE_OWNER_PAYLOAD, PUBLIC_TEST_PREMINE_OWNER_PAYLOAD);
    }

    /// audit H-01 launch-safety guard: the [`MAINNET_PREMINE_CEREMONY_PENDING`] flag MUST
    /// agree with the actual payload state — `true` IFF the mainnet payload is still the
    /// all-zero unspendable placeholder. This catches the two release mistakes the ceremony
    /// could make: shipping a real ceremony payload while the flag still says "pending"
    /// (the runbook would skip the re-genesis steps), or flipping the flag to "done" while
    /// the payload is still the unspendable placeholder (a permanently locked 15B premine).
    #[test]
    fn mainnet_ceremony_pending_flag_matches_placeholder_state() {
        let is_placeholder = MAINNET_PREMINE_OWNER_PAYLOAD == [0u8; 64];
        assert_eq!(
            MAINNET_PREMINE_CEREMONY_PENDING, is_placeholder,
            "MAINNET_PREMINE_CEREMONY_PENDING ({MAINNET_PREMINE_CEREMONY_PENDING}) must equal 'payload is the all-zero placeholder' ({is_placeholder}) — flip the flag in the same change that installs the ceremony payload"
        );
    }

    /// audit H-01 ceremony tool — turn the OFFLINE-generated custody public key into the exact
    /// re-genesis values, so the mainnet premine ceremony is mechanical (and the private key never
    /// touches this repo). Feed the ceremony's ML-DSA-87 **public** key (2592 bytes) via the
    /// `CEREMONY_PUBKEY_HEX` env var and run:
    ///   `CEREMONY_PUBKEY_HEX=<hex> cargo test -p kaspa-consensus-core --lib \
    ///      config::premine::tests::mainnet_premine_regenesis_from_ceremony_pubkey -- --ignored --nocapture`
    /// It prints (1) the 64-byte `MAINNET_PREMINE_OWNER_PAYLOAD` to paste in, (2) the mainnet
    /// address, and (3) the new `MAINNET_PREMINE_UTXO_COMMITMENT` for `genesis.rs` — after which the
    /// operator recomputes `GENESIS.hash` and flips `MAINNET_PREMINE_CEREMONY_PENDING` to `false`.
    /// With no env var it SELF-TESTS the derivation against a deterministic sample key (no I/O, no key
    /// persisted), so the pipeline is proven correct without exposing any custody material.
    #[test]
    #[ignore]
    fn mainnet_premine_regenesis_from_ceremony_pubkey() {
        use kaspa_addresses::{Address, Prefix, Version};
        use kaspa_hashes::blake2b_512_address_payload;
        use libcrux_ml_dsa::ml_dsa_87;

        // The pubkey-→re-genesis derivation, identical to the genesis path.
        let regenesis = |pubkey: &[u8]| -> ([u8; 64], Hash64, String) {
            assert_eq!(pubkey.len(), 2592, "ML-DSA-87 public key must be 2592 bytes");
            let payload: [u8; 64] = blake2b_512_address_payload(pubkey).as_bytes();
            let spk = crate::dns_finality::p2pkh_mldsa87_spk(&payload);
            assert_eq!(spk.script().len(), 69, "ML-DSA P2PKH scriptPubKey must be 69 bytes");
            // utxo_commitment = MuHash over the single premine UTXO locked to this payload.
            let outpoint = TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index: 0 };
            let entry = UtxoEntry { amount: MISAKA_PREMINE_SOMPI, script_public_key: spk, block_daa_score: 0, is_coinbase: false };
            let mut ms = MuHash::new();
            ms.add_utxo(&outpoint, &entry);
            let commitment = ms.finalize();
            let address = Address::new(Prefix::Mainnet, Version::PubKeyHashMlDsa87, &payload).to_string();
            (payload, commitment, address)
        };

        match std::env::var("CEREMONY_PUBKEY_HEX") {
            Ok(hex) => {
                let pubkey: Vec<u8> = (0..hex.trim().len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex.trim()[i..i + 2], 16).expect("CEREMONY_PUBKEY_HEX is hex"))
                    .collect();
                let (payload, commitment, address) = regenesis(&pubkey);
                let payload_rust = payload.iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
                let commit_rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
                println!("=== MAINNET PREMINE CEREMONY RE-GENESIS (audit H-01) ===");
                println!("// 1. config/premine.rs — replace MAINNET_PREMINE_OWNER_PAYLOAD with:");
                println!("const MAINNET_PREMINE_OWNER_PAYLOAD: [u8; 64] = [{payload_rust}];");
                println!("// 2. config/premine.rs — set: pub const MAINNET_PREMINE_CEREMONY_PENDING: bool = false;");
                println!("// 3. config/genesis.rs — set Mainnet utxo_commitment:");
                println!("Hash64::from_bytes([{commit_rust}])");
                println!("// 4. recompute GENESIS.hash (re-run the genesis-hash test) and the mainnet address is:");
                println!("MAINNET_ADDRESS: {address}");
            }
            Err(_) => {
                // No custody key supplied: self-test the derivation on a deterministic sample so the
                // tool is proven correct. The sample key is NOT a custody key and is never persisted.
                let sample = ml_dsa_87::generate_key_pair([0x5au8; 32]);
                let (payload, commitment, address) = regenesis(sample.verification_key.as_ref());
                assert_ne!(payload, [0u8; 64], "a real key never derives the all-zero placeholder payload");
                assert_ne!(payload, PUBLIC_TEST_PREMINE_OWNER_PAYLOAD, "sample differs from the public test key");
                assert_ne!(commitment, Hash64::default(), "commitment is well-defined");
                assert!(address.starts_with("misaka:"), "mainnet address renders with the misaka prefix ({address})");
                println!("SELF-TEST OK (no CEREMONY_PUBKEY_HEX): set it to your offline ML-DSA-87 pubkey hex to emit the real re-genesis values.");
            }
        }
    }

    /// Deterministically derives the **PUBLIC** test-network ML-DSA-87 premine
    /// keypair and prints its 64-byte BLAKE2b-512 owner payload (the value baked
    /// into [`PUBLIC_TEST_PREMINE_OWNER_PAYLOAD`]) and the devnet address. The key
    /// is public — anyone can reproduce it from the seed string — and is for
    /// value-less testnet/devnet/simnet ONLY, NEVER mainnet. Prints only; it
    /// deliberately does NOT write any key file to the repo (audit H-01).
    /// Run (ignored by default):
    /// `cargo test -p kaspa-consensus-core --lib config::premine::tests::gen_public_test_premine_key -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn gen_public_test_premine_key() {
        use blake2b_simd::Params;
        use kaspa_addresses::{Address, Prefix, Version};
        use kaspa_hashes::blake2b_512_address_payload;
        use libcrux_ml_dsa::ml_dsa_87;

        // Documented PUBLIC deterministic test seed: BLAKE2b-256("misaka-devnet-premine-key").
        let seed_hash = Params::new().hash_length(32).hash(b"misaka-devnet-premine-key");
        let mut seed = [0u8; 32];
        seed.copy_from_slice(seed_hash.as_bytes());

        let key_pair = ml_dsa_87::generate_key_pair(seed);
        let pubkey = key_pair.verification_key.as_ref();

        // Owner payload = keyed BLAKE2b-512(public key) under `kaspa-pq-v2/address/mldsa87`.
        let payload: [u8; 64] = blake2b_512_address_payload(pubkey).as_bytes();

        // Standard ML-DSA P2PKH scriptPubKey must be exactly 69 bytes.
        let spk = crate::dns_finality::p2pkh_mldsa87_spk(&payload);
        assert_eq!(spk.script().len(), 69, "ML-DSA P2PKH scriptPubKey must be 69 bytes");

        let payload_rust = payload.iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        let devnet_address = Address::new(Prefix::Devnet, Version::PubKeyHashMlDsa87, &payload).to_string();
        println!("PUBLIC_TEST_PREMINE_OWNER_PAYLOAD: [{payload_rust}]");
        println!("DEVNET_ADDRESS: {devnet_address}");
        // audit H-01: intentionally does NOT persist a key file. This key is public
        // and for test networks only; never write it out as if it were custody material.
    }
}
