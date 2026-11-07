//!
//! Utilities and helpers for unit and integration testing.
//!

#[cfg(test)]
mod rpc_core_mock;
pub use rpc_core_mock::*;

// kaspa-pq PQ-only (ADR-0019 §14): the `make_xpub` test helper produces a classical
// secp256k1 extended public key, consumed only by the classical account-variant tests.
#[cfg(feature = "legacy-secp256k1")]
mod keys;
#[cfg(feature = "legacy-secp256k1")]
pub use keys::*;

mod storage;
pub use storage::*;
