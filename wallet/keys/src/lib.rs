//!
//! # Kaspa Wallet Keys
//!
//! This crate provides tools for creating and managing Kaspa wallet keys.
//! This includes extended key generation and derivation.
//!

#[cfg(feature = "legacy-secp256k1")]
pub mod derivation;
pub mod derivation_path;
pub mod error;
mod imports;
/// kaspa-pq Phase 5: ML-DSA-87 wallet keygen + P2PKH address derivation
/// (see docs/kaspa-pq-spec.md §8 and docs/adr/0002-mldsa65-p2pkh.md).
pub mod kaspa_pq;

/// kaspa-pq Phase 7 (PR-7.4): WASM bindings for the kaspa-pq
/// cryptographic primitives. See docs/adr/0006-rpc-wasm-sdk-types.md §4.
///
/// kaspa-pq PR-19-S5e: also exposed under the `test-utils` feature so native
/// test builds of downstream crates (kaspa-wallet-core) can reach
/// `KaspaPqKeyPair` for the WASM-vs-native signer parity assertion. The module
/// compiles cleanly off-wasm (it only delegates to the native ML-DSA keypair).
#[cfg(any(target_arch = "wasm32", test, feature = "test-utils"))]
pub mod kaspa_pq_wasm;
#[cfg(feature = "legacy-secp256k1")]
pub mod keypair;
pub mod prelude;
#[cfg(feature = "legacy-secp256k1")]
pub mod privatekey;
#[cfg(feature = "legacy-secp256k1")]
pub mod privkeygen;
#[cfg(feature = "legacy-secp256k1")]
pub mod pubkeygen;
#[cfg(feature = "legacy-secp256k1")]
pub mod publickey;
pub mod result;
pub mod secret;
pub mod types;
#[cfg(feature = "legacy-secp256k1")]
pub mod xprv;
#[cfg(feature = "legacy-secp256k1")]
pub mod xpub;
