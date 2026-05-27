//!
//! # Kaspa Wallet Keys
//!
//! This crate provides tools for creating and managing Kaspa wallet keys.
//! This includes extended key generation and derivation.
//!

pub mod derivation;
pub mod derivation_path;
pub mod error;
mod imports;
/// kaspa-pq Phase 5: ML-DSA-65 wallet keygen + P2PKH address derivation
/// (see docs/kaspa-pq-spec.md §8 and docs/adr/0002-mldsa65-p2pkh.md).
pub mod kaspa_pq;

/// kaspa-pq Phase 7 (PR-7.4): WASM bindings for the kaspa-pq
/// cryptographic primitives. See docs/adr/0006-rpc-wasm-sdk-types.md §4.
#[cfg(any(target_arch = "wasm32", test))]
pub mod kaspa_pq_wasm;
pub mod keypair;
pub mod prelude;
pub mod privatekey;
pub mod privkeygen;
pub mod pubkeygen;
pub mod publickey;
pub mod result;
pub mod secret;
pub mod types;
pub mod xprv;
pub mod xpub;
