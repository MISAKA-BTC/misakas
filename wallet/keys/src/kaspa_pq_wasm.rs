//! kaspa-pq Phase 7 (PR-7.4): WASM bindings for the kaspa-pq
//! cryptographic primitives.
//!
//! TypeScript-facing API (camelCased automatically by wasm-bindgen):
//!
//! ```text
//!   class MlDsa87PublicKey {
//!     static fromHex(hex: string): MlDsa87PublicKey;
//!     static fromBytes(bytes: Uint8Array): MlDsa87PublicKey;
//!     toHex(): string;
//!     toBytes(): Uint8Array;       // length 2592
//!   }
//!   class MlDsa87Signature {
//!     static fromHex(hex: string): MlDsa87Signature;
//!     static fromBytes(bytes: Uint8Array): MlDsa87Signature;
//!     toHex(): string;
//!     toBytes(): Uint8Array;       // length 4627
//!     verify(publicKey: MlDsa87PublicKey, message: Uint8Array): boolean;
//!   }
//!   class KaspaPqKeyPair {
//!     static fromSeed(seed: Uint8Array): KaspaPqKeyPair;     // 32-byte seed
//!     static fromMnemonic(phrase, passphrase, networkId,
//!                          account, change, index): KaspaPqKeyPair;
//!     publicKey(): MlDsa87PublicKey;
//!     address(networkPrefix: string): Address;
//!     sign(message: Uint8Array, randomness: Uint8Array): MlDsa87Signature;
//!   }
//! ```
//!
//! `Address` is the existing kaspa-addresses WASM class; `networkId` and
//! `networkPrefix` use the kaspa-pq prefix family (`misaka`,
//! `misakatest`, `misakasim`, `misakadev`).
//!
//! Internally each `Result<_, JsValue>`-returning method delegates to a
//! private `_inner` method that returns `Result<_, String>`. That keeps the
//! happy-path and the error-message construction unit-testable on native
//! targets — `wasm_bindgen::JsValue::from_str` panics off-wasm. The
//! `JsValue` wrap is only performed at the wasm-bindgen boundary.
//!
//! See docs/adr/0006-rpc-wasm-sdk-types.md §4 for the design contract.

use kaspa_addresses::{Address, Prefix};
use kaspa_bip32::{Language, Mnemonic};
use kaspa_txscript::{MLDSA87_PK_LEN, MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT};
use libcrux_ml_dsa::ml_dsa_87;
use wasm_bindgen::prelude::*;

use crate::kaspa_pq::{KaspaPqMlDsa87KeyPair, derive_keypair};

fn require_len(bytes: &[u8], expected: usize, label: &str) -> Result<(), String> {
    if bytes.len() != expected { Err(format!("kaspa-pq {label}: expected {expected} bytes, got {}", bytes.len())) } else { Ok(()) }
}

fn jsv<E: std::fmt::Display>(e: E) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// 2592-byte ML-DSA-87 public key, WASM-facing newtype.
#[derive(Debug, Clone, PartialEq, Eq)]
#[wasm_bindgen(js_name = "MlDsa87PublicKey")]
pub struct MlDsa87PublicKey {
    inner: Vec<u8>,
}

impl MlDsa87PublicKey {
    fn from_bytes_inner(bytes: Vec<u8>) -> Result<MlDsa87PublicKey, String> {
        require_len(&bytes, MLDSA87_PK_LEN, "MlDsa87PublicKey")?;
        Ok(MlDsa87PublicKey { inner: bytes })
    }

    fn from_hex_inner(hex: &str) -> Result<MlDsa87PublicKey, String> {
        if hex.len() != MLDSA87_PK_LEN * 2 {
            return Err(format!("kaspa-pq MlDsa87PublicKey: expected {} hex characters, got {}", MLDSA87_PK_LEN * 2, hex.len()));
        }
        let mut buf = vec![0u8; MLDSA87_PK_LEN];
        faster_hex::hex_decode(hex.as_bytes(), &mut buf).map_err(|e| format!("kaspa-pq MlDsa87PublicKey hex: {e}"))?;
        Ok(MlDsa87PublicKey { inner: buf })
    }
}

#[wasm_bindgen(js_class = "MlDsa87PublicKey")]
impl MlDsa87PublicKey {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<MlDsa87PublicKey, JsValue> {
        Self::from_bytes_inner(bytes).map_err(jsv)
    }

    #[wasm_bindgen(js_name = "fromHex")]
    pub fn from_hex(hex: &str) -> Result<MlDsa87PublicKey, JsValue> {
        Self::from_hex_inner(hex).map_err(jsv)
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.clone()
    }

    #[wasm_bindgen(js_name = "toHex")]
    pub fn to_hex(&self) -> String {
        let mut out = vec![0u8; self.inner.len() * 2];
        faster_hex::hex_encode(&self.inner, &mut out).expect("hex encode");
        String::from_utf8(out).expect("hex output is ASCII")
    }
}

/// 4627-byte ML-DSA-87 signature, WASM-facing newtype.
#[derive(Debug, Clone, PartialEq, Eq)]
#[wasm_bindgen(js_name = "MlDsa87Signature")]
pub struct MlDsa87Signature {
    inner: Vec<u8>,
}

impl MlDsa87Signature {
    fn from_bytes_inner(bytes: Vec<u8>) -> Result<MlDsa87Signature, String> {
        require_len(&bytes, MLDSA87_SIG_LEN, "MlDsa87Signature")?;
        Ok(MlDsa87Signature { inner: bytes })
    }

    fn from_hex_inner(hex: &str) -> Result<MlDsa87Signature, String> {
        if hex.len() != MLDSA87_SIG_LEN * 2 {
            return Err(format!("kaspa-pq MlDsa87Signature: expected {} hex characters, got {}", MLDSA87_SIG_LEN * 2, hex.len()));
        }
        let mut buf = vec![0u8; MLDSA87_SIG_LEN];
        faster_hex::hex_decode(hex.as_bytes(), &mut buf).map_err(|e| format!("kaspa-pq MlDsa87Signature hex: {e}"))?;
        Ok(MlDsa87Signature { inner: buf })
    }

    fn verify_inner(&self, public_key: &MlDsa87PublicKey, message: &[u8]) -> bool {
        if public_key.inner.len() != MLDSA87_PK_LEN || self.inner.len() != MLDSA87_SIG_LEN {
            return false;
        }
        let Ok(pk_arr): Result<[u8; MLDSA87_PK_LEN], _> = public_key.inner.as_slice().try_into() else {
            return false;
        };
        let Ok(sig_arr): Result<[u8; MLDSA87_SIG_LEN], _> = self.inner.as_slice().try_into() else {
            return false;
        };
        let vk = ml_dsa_87::MLDSA87VerificationKey::new(pk_arr);
        let sig = ml_dsa_87::MLDSA87Signature::new(sig_arr);
        ml_dsa_87::verify(&vk, message, MLDSA87_TX_CONTEXT, &sig).is_ok()
    }
}

#[wasm_bindgen(js_class = "MlDsa87Signature")]
impl MlDsa87Signature {
    #[wasm_bindgen(js_name = "fromBytes")]
    pub fn from_bytes(bytes: Vec<u8>) -> Result<MlDsa87Signature, JsValue> {
        Self::from_bytes_inner(bytes).map_err(jsv)
    }

    #[wasm_bindgen(js_name = "fromHex")]
    pub fn from_hex(hex: &str) -> Result<MlDsa87Signature, JsValue> {
        Self::from_hex_inner(hex).map_err(jsv)
    }

    #[wasm_bindgen(js_name = "toBytes")]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.clone()
    }

    #[wasm_bindgen(js_name = "toHex")]
    pub fn to_hex(&self) -> String {
        let mut out = vec![0u8; self.inner.len() * 2];
        faster_hex::hex_encode(&self.inner, &mut out).expect("hex encode");
        String::from_utf8(out).expect("hex output is ASCII")
    }

    /// Verify under the kaspa-pq tx context `MLDSA87_TX_CONTEXT`. Self-contained;
    /// does not require a `KaspaPqKeyPair`. Returns `true` for a valid signature,
    /// `false` for any failure (length / context / cryptographic).
    pub fn verify(&self, public_key: &MlDsa87PublicKey, message: Vec<u8>) -> bool {
        self.verify_inner(public_key, &message)
    }
}

/// kaspa-pq ML-DSA-87 keypair, WASM-facing.
#[wasm_bindgen(js_name = "KaspaPqKeyPair")]
pub struct KaspaPqKeyPair {
    inner: KaspaPqMlDsa87KeyPair,
}

// `KaspaPqMlDsa87KeyPair` does not derive Debug (it contains the libcrux
// signing key), and we do not want test code to print private key
// material anyway. Implement Debug as a redacted form for the WASM
// wrapper.
impl std::fmt::Debug for KaspaPqKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KaspaPqKeyPair").field("public_key", &self.inner.public_key_bytes().len()).finish_non_exhaustive()
    }
}

impl KaspaPqKeyPair {
    fn from_seed_inner(seed: Vec<u8>) -> Result<KaspaPqKeyPair, String> {
        require_len(&seed, 32, "fromSeed")?;
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&seed);
        Ok(KaspaPqKeyPair { inner: KaspaPqMlDsa87KeyPair::from_seed(arr) })
    }

    fn from_mnemonic_inner(
        phrase: &str,
        passphrase: &str,
        network_id: &str,
        account: u32,
        change: u32,
        index: u32,
    ) -> Result<KaspaPqKeyPair, String> {
        let mnemonic = Mnemonic::new(phrase, Language::English).map_err(|e| format!("kaspa-pq BIP39: {e}"))?;
        let seed = mnemonic.to_seed(passphrase);
        Ok(KaspaPqKeyPair { inner: derive_keypair(network_id, account, change, index, seed.as_bytes()) })
    }

    fn public_key_inner(&self) -> MlDsa87PublicKey {
        MlDsa87PublicKey { inner: self.inner.public_key_bytes().to_vec() }
    }

    fn address_inner(&self, network_prefix: &str) -> Result<Address, String> {
        // Only the kaspa-pq prefix family is accepted by the JS API. The
        // test-only `a`/`b` synthetic prefixes from kaspa_addresses are
        // intentionally not part of the kaspa-pq wire vocabulary.
        let prefix = match network_prefix {
            "misaka" => Prefix::Mainnet,
            "misakatest" => Prefix::Testnet,
            "misakasim" => Prefix::Simnet,
            "misakadev" => Prefix::Devnet,
            other => {
                return Err(format!("kaspa-pq KaspaPqKeyPair.address: unsupported network prefix '{other}'"));
            }
        };
        Ok(self.inner.address(prefix))
    }

    fn sign_inner(&self, message: &[u8], randomness: Vec<u8>) -> Result<MlDsa87Signature, String> {
        require_len(&randomness, 32, "sign randomness")?;
        let mut r_arr = [0u8; 32];
        r_arr.copy_from_slice(&randomness);
        let sig_bytes = self.inner.sign(message, r_arr);
        Ok(MlDsa87Signature { inner: sig_bytes.to_vec() })
    }
}

#[wasm_bindgen(js_class = "KaspaPqKeyPair")]
impl KaspaPqKeyPair {
    /// Build a keypair directly from a 32-byte seed. Caller is responsible
    /// for deriving the seed via a domain-separated XOF; see
    /// [`KaspaPqKeyPair.fromMnemonic`] for the kaspa-pq-spec'd path.
    #[wasm_bindgen(js_name = "fromSeed")]
    pub fn from_seed(seed: Vec<u8>) -> Result<KaspaPqKeyPair, JsValue> {
        Self::from_seed_inner(seed).map_err(jsv)
    }

    /// Derive a keypair from a BIP39 mnemonic + kaspa-pq derivation path,
    /// matching the native Rust `kaspa_wallet_keys::kaspa_pq::derive_keypair`.
    ///
    /// `networkId` is the kaspa-pq `NetworkId::to_string` form, e.g.
    /// "mainnet", "testnet-10", "simnet", "devnet".
    #[wasm_bindgen(js_name = "fromMnemonic")]
    pub fn from_mnemonic(
        phrase: &str,
        passphrase: &str,
        network_id: &str,
        account: u32,
        change: u32,
        index: u32,
    ) -> Result<KaspaPqKeyPair, JsValue> {
        Self::from_mnemonic_inner(phrase, passphrase, network_id, account, change, index).map_err(jsv)
    }

    /// 2592-byte ML-DSA-87 public key.
    #[wasm_bindgen(js_name = "publicKey")]
    pub fn public_key(&self) -> MlDsa87PublicKey {
        self.public_key_inner()
    }

    /// kaspa-pq P2PKH address for the given prefix string. Accepts the
    /// kaspa-pq prefix family (`misaka`, `misakatest`, `misakasim`,
    /// `misakadev`).
    pub fn address(&self, network_prefix: &str) -> Result<Address, JsValue> {
        self.address_inner(network_prefix).map_err(jsv)
    }

    /// Sign `message` under the kaspa-pq transaction context. The
    /// returned signature is exactly 4627 bytes.
    pub fn sign(&self, message: Vec<u8>, randomness: Vec<u8>) -> Result<MlDsa87Signature, JsValue> {
        self.sign_inner(&message, randomness).map_err(jsv)
    }
}

#[cfg(test)]
mod tests {
    //! These tests exercise the Rust-side logic of the WASM bindings via
    //! the private `_inner` methods, so they can run on native targets
    //! (`JsValue::from_str` panics off-wasm). In-browser coverage via
    //! `wasm_bindgen_test` is a separate test corpus.

    use super::*;

    const TEST_MASTER_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn keypair_roundtrip_from_mnemonic() {
        let kp = KaspaPqKeyPair::from_mnemonic_inner(TEST_MASTER_PHRASE, "", "mainnet", 0, 0, 0).unwrap();
        let pk = kp.public_key_inner();
        assert_eq!(pk.to_bytes().len(), MLDSA87_PK_LEN);
        let hex = pk.to_hex();
        assert_eq!(hex.len(), MLDSA87_PK_LEN * 2);

        let pk_back = MlDsa87PublicKey::from_hex_inner(&hex).unwrap();
        assert_eq!(pk_back.to_bytes(), pk.to_bytes());
    }

    #[test]
    fn address_uses_misaka_prefix() {
        let kp = KaspaPqKeyPair::from_mnemonic_inner(TEST_MASTER_PHRASE, "", "mainnet", 0, 0, 0).unwrap();
        let mainnet = kp.address_inner("misaka").unwrap();
        let mainnet_str: String = mainnet.into();
        assert!(mainnet_str.starts_with("misaka:"), "got {mainnet_str}");

        let testnet = kp.address_inner("misakatest").unwrap();
        let testnet_str: String = testnet.into();
        assert!(testnet_str.starts_with("misakatest:"), "got {testnet_str}");

        let simnet = kp.address_inner("misakasim").unwrap();
        let simnet_str: String = simnet.into();
        assert!(simnet_str.starts_with("misakasim:"), "got {simnet_str}");

        let devnet = kp.address_inner("misakadev").unwrap();
        let devnet_str: String = devnet.into();
        assert!(devnet_str.starts_with("misakadev:"), "got {devnet_str}");
    }

    #[test]
    fn address_rejects_legacy_prefixes() {
        let kp = KaspaPqKeyPair::from_mnemonic_inner(TEST_MASTER_PHRASE, "", "mainnet", 0, 0, 0).unwrap();
        // Legacy upstream Kaspa prefixes — both must be rejected.
        assert!(kp.address_inner("kaspa").is_err());
        assert!(kp.address_inner("kaspatest").is_err());
        assert!(kp.address_inner("").is_err());
        assert!(kp.address_inner("not-a-prefix").is_err());
    }

    #[test]
    fn sign_then_verify_roundtrip() {
        let kp = KaspaPqKeyPair::from_mnemonic_inner(TEST_MASTER_PHRASE, "", "simnet", 0, 0, 0).unwrap();
        let pk = kp.public_key_inner();
        let message = b"kaspa-pq Phase 7 PR-7.4 wasm bindings smoke test".to_vec();
        let sig = kp.sign_inner(&message, vec![0x77u8; 32]).unwrap();
        assert_eq!(sig.to_bytes().len(), MLDSA87_SIG_LEN);
        assert!(sig.verify_inner(&pk, &message));
    }

    #[test]
    fn signature_does_not_verify_under_tampered_message() {
        let kp = KaspaPqKeyPair::from_mnemonic_inner(TEST_MASTER_PHRASE, "", "simnet", 0, 0, 0).unwrap();
        let pk = kp.public_key_inner();
        let original = b"original".to_vec();
        let tampered = b"tampered".to_vec();
        let sig = kp.sign_inner(&original, vec![0x88u8; 32]).unwrap();
        assert!(sig.verify_inner(&pk, &original));
        assert!(!sig.verify_inner(&pk, &tampered));
    }

    #[test]
    fn wrong_length_inputs_are_rejected_with_clear_messages() {
        let err = MlDsa87PublicKey::from_bytes_inner(vec![0u8; 100]).unwrap_err();
        assert!(err.contains("expected 2592 bytes, got 100"), "got {err}");

        let err = MlDsa87PublicKey::from_hex_inner("00").unwrap_err();
        assert!(err.contains("5184 hex characters"), "got {err}");

        let err = MlDsa87Signature::from_bytes_inner(vec![0u8; 100]).unwrap_err();
        assert!(err.contains("expected 4627 bytes, got 100"), "got {err}");

        let err = KaspaPqKeyPair::from_seed_inner(vec![0u8; 16]).unwrap_err();
        assert!(err.contains("expected 32 bytes, got 16"), "got {err}");

        let kp = KaspaPqKeyPair::from_seed_inner(vec![0xaa; 32]).unwrap();
        let err = kp.sign_inner(b"any", vec![0u8; 16]).unwrap_err();
        assert!(err.contains("expected 32 bytes, got 16"), "got {err}");
    }
}
