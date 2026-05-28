//! kaspa-pq Phase 5: ML-DSA-65 wallet key derivation.
//!
//! BIP32-style hierarchical key derivation assumes a discrete-log-friendly
//! curve (secp256k1) and is therefore unavailable for an ML-DSA-65 wallet.
//! kaspa-pq replaces it with a domain-separated XOF keyed by the BIP39
//! master seed:
//!
//! ```text
//!   keygen_seed =
//!       BLAKE2b-256(
//!           key   = b"kaspa-pq-wallet-v1/mldsa65/keygen",
//!           input = network_id || account_le || change_le || index_le || master_seed,
//!       )
//!   (verification_key, signing_key) = ML-DSA-65.KeyGen(keygen_seed)
//!   address = (prefix, Version::PubKeyHashMlDsa65, BLAKE2b-256(verification_key))
//! ```
//!
//! See docs/kaspa-pq-spec.md §8 for the normative spec. Phase 5 keeps the
//! derivation deterministic and side-effect free; persistent storage of
//! the master seed and the wallet-CLI plumbing
//! (`create`/`show-address`/`build-tx`/`sign-tx`/`submit-tx`) are
//! follow-ups on top of this module.

use blake2b_simd::Params;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_txscript::{MLDSA65_PK_LEN, MLDSA65_SIG_LEN, MLDSA65_TX_CONTEXT};
use libcrux_ml_dsa::ml_dsa_65;

/// Domain separator for the kaspa-pq wallet keygen XOF. Used as the BLAKE2b
/// key (max 64 bytes; this string is 33 bytes).
pub const KASPA_PQ_WALLET_KEYGEN_DOMAIN: &[u8] = b"kaspa-pq-wallet-v1/mldsa65/keygen";

/// kaspa-pq ML-DSA-65 wallet keypair, deterministically derived from a
/// 32-byte `keygen_seed` (see [`derive_keygen_seed`]).
pub struct KaspaPqMlDsa65KeyPair {
    inner: ml_dsa_65::MLDSA65KeyPair,
}

impl KaspaPqMlDsa65KeyPair {
    /// Build a fresh keypair from a 32-byte deterministic seed. The seed
    /// should come from [`derive_keygen_seed`] in production paths so the
    /// address can be recomputed from the BIP39 mnemonic + account/index
    /// alone.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self { inner: ml_dsa_65::generate_key_pair(seed) }
    }

    /// 1952-byte ML-DSA-65 public key bytes. This is exactly
    /// `MLDSA65_PK_LEN` long.
    pub fn public_key_bytes(&self) -> &[u8; MLDSA65_PK_LEN] {
        // The libcrux constants match ours by construction (Phase 1 spec).
        self.inner.verification_key.as_ref()
    }

    /// 32-byte address payload: `BLAKE2b-256(public_key)`.
    pub fn public_key_hash(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(Params::new().hash_length(32).to_state().update(self.public_key_bytes()).finalize().as_bytes());
        out
    }

    /// kaspa-pq P2PKH `Address` for the given network prefix.
    pub fn address(&self, prefix: Prefix) -> Address {
        Address::new(prefix, Version::PubKeyHashMlDsa65, &self.public_key_hash())
    }

    /// Sign an arbitrary message with the kaspa-pq transaction context
    /// ([`MLDSA65_TX_CONTEXT`]). Returns the 3309-byte signature bytes.
    ///
    /// The caller is responsible for choosing `message` correctly — for a
    /// transaction input that means the sighash digest from
    /// `kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash`.
    /// `signing_randomness` is 32 bytes of fresh randomness per signature;
    /// reusing it across signatures is **not** required for ML-DSA security
    /// (the scheme is hedged-randomized), but reusing the *same* signing
    /// key with predictable randomness is bad hygiene.
    pub fn sign(&self, message: &[u8], signing_randomness: [u8; 32]) -> [u8; MLDSA65_SIG_LEN] {
        let sig = ml_dsa_65::sign(&self.inner.signing_key, message, MLDSA65_TX_CONTEXT, signing_randomness)
            .expect("ML-DSA-65 sign is infallible on a well-formed message");
        // `MLDSA65Signature::as_ref()` returns `&[u8; SIGNATURE_SIZE]`.
        *sig.as_ref()
    }
}

/// Derive the 32-byte ML-DSA-65 keygen seed from BIP39-style inputs.
///
/// Inputs are mixed via a keyed BLAKE2b-256 with
/// [`KASPA_PQ_WALLET_KEYGEN_DOMAIN`] as the key. The exact wire form is:
///
/// ```text
///   keyed_blake2b_256(
///       key   = KASPA_PQ_WALLET_KEYGEN_DOMAIN,
///       input = network_id_bytes || account_le_u32 || change_le_u32 || index_le_u32 || master_seed,
///   )
/// ```
///
/// `network_id` is the kaspa-pq [`NetworkId::to_string`] form
/// (`"mainnet"`, `"testnet-10"`, etc.) so that the same BIP39 mnemonic on
/// mainnet and testnet produces distinct addresses. The encoded length is
/// included implicitly via the trailing master_seed (BLAKE2b-256 is
/// collision-resistant, so the lack of an explicit length tag is fine
/// here).
pub fn derive_keygen_seed(network_id: &str, account: u32, change: u32, index: u32, master_seed: &[u8]) -> [u8; 32] {
    let mut state = Params::new().hash_length(32).key(KASPA_PQ_WALLET_KEYGEN_DOMAIN).to_state();
    state.update(network_id.as_bytes());
    state.update(&account.to_le_bytes());
    state.update(&change.to_le_bytes());
    state.update(&index.to_le_bytes());
    state.update(master_seed);
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// One-shot helper: derive a keygen seed and materialise the keypair.
pub fn derive_keypair(network_id: &str, account: u32, change: u32, index: u32, master_seed: &[u8]) -> KaspaPqMlDsa65KeyPair {
    KaspaPqMlDsa65KeyPair::from_seed(derive_keygen_seed(network_id, account, change, index, master_seed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed BIP39-style 64-byte master seed (placeholder for
    /// tests — production paths derive this from
    /// `kaspa_bip32::Mnemonic::to_seed`).
    const TEST_MASTER_SEED: [u8; 64] = [0xab; 64];

    #[test]
    fn derivation_is_deterministic() {
        let a = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let b = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        assert_eq!(a, b);
    }

    #[test]
    fn network_id_separates_keys() {
        let mainnet = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let testnet = derive_keygen_seed("testnet-10", 0, 0, 0, &TEST_MASTER_SEED);
        assert_ne!(mainnet, testnet);
    }

    #[test]
    fn index_separates_keys() {
        let i0 = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let i1 = derive_keygen_seed("mainnet", 0, 0, 1, &TEST_MASTER_SEED);
        assert_ne!(i0, i1);
    }

    #[test]
    fn account_separates_keys() {
        let a0 = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let a1 = derive_keygen_seed("mainnet", 1, 0, 0, &TEST_MASTER_SEED);
        assert_ne!(a0, a1);
    }

    #[test]
    fn change_separates_keys() {
        let receive = derive_keygen_seed("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        let change = derive_keygen_seed("mainnet", 0, 1, 0, &TEST_MASTER_SEED);
        assert_ne!(receive, change);
    }

    #[test]
    fn keypair_round_trip_and_address_shape() {
        let kp = derive_keypair("mainnet", 0, 0, 0, &TEST_MASTER_SEED);
        assert_eq!(kp.public_key_bytes().len(), MLDSA65_PK_LEN);

        let mainnet = kp.address(Prefix::Mainnet);
        let s: String = mainnet.into();
        assert!(s.starts_with("kaspapq:"), "got {s}");

        let testnet = kp.address(Prefix::Testnet);
        let s_tn: String = testnet.into();
        assert!(s_tn.starts_with("kaspapqtest:"), "got {s_tn}");
    }

    #[test]
    fn sign_and_locally_verify() {
        // Sanity check that a signature produced by `KaspaPqMlDsa65KeyPair::sign`
        // verifies under `libcrux_ml_dsa::ml_dsa_65::verify` with the
        // kaspa-pq context. (The script engine's hash-keyed
        // `check_mldsa65_signature` is tested end-to-end in
        // `kaspa-txscript`'s `test_mldsa65_p2pkh_spend_roundtrip`.)
        let kp = derive_keypair("simnet", 0, 0, 7, &TEST_MASTER_SEED);
        let msg = b"kaspa-pq Phase 5 wallet derivation smoke test";
        let randomness = [0x33u8; 32];
        let sig_bytes = kp.sign(msg, randomness);
        assert_eq!(sig_bytes.len(), MLDSA65_SIG_LEN);

        let vk = libcrux_ml_dsa::ml_dsa_65::MLDSA65VerificationKey::new(*kp.public_key_bytes());
        let sig = libcrux_ml_dsa::ml_dsa_65::MLDSA65Signature::new(sig_bytes);
        libcrux_ml_dsa::ml_dsa_65::verify(&vk, msg, MLDSA65_TX_CONTEXT, &sig)
            .expect("kaspa-pq wallet signature must verify under the kaspa-pq tx context");
    }

    #[test]
    fn signature_does_not_verify_under_wrong_context() {
        let kp = derive_keypair("simnet", 0, 0, 1, &TEST_MASTER_SEED);
        let msg = b"context-binding test";
        let sig_bytes = kp.sign(msg, [0x11u8; 32]);
        let vk = libcrux_ml_dsa::ml_dsa_65::MLDSA65VerificationKey::new(*kp.public_key_bytes());
        let sig = libcrux_ml_dsa::ml_dsa_65::MLDSA65Signature::new(sig_bytes);
        // Wrong context => verify must reject.
        assert!(
            libcrux_ml_dsa::ml_dsa_65::verify(&vk, msg, b"not-the-kaspa-pq-context", &sig).is_err(),
            "ML-DSA must reject under a different ctx — domain separation is the whole point",
        );
    }
}
