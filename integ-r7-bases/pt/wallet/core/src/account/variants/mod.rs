//!
//! Kaspa core wallet account variant implementations.
//!

// kaspa-pq PQ-only (ADR-0019 §14): the classical secp256k1 account variants are
// gated; only the ML-DSA (`mldsa`) PQ account and the in-memory `resident`
// account remain in the PQ-only build.
#[cfg(feature = "legacy-secp256k1")]
pub mod bip32;
#[cfg(feature = "legacy-secp256k1")]
pub mod bip32watch;
#[cfg(feature = "legacy-secp256k1")]
pub mod keypair;
#[cfg(feature = "legacy-secp256k1")]
pub mod legacy;
pub mod mldsa;
#[cfg(feature = "legacy-secp256k1")]
pub mod multisig;
#[cfg(feature = "legacy-secp256k1")]
pub mod resident;

#[cfg(feature = "legacy-secp256k1")]
pub use bip32::BIP32_ACCOUNT_KIND;
#[cfg(feature = "legacy-secp256k1")]
pub use bip32watch::BIP32_WATCH_ACCOUNT_KIND;
#[cfg(feature = "legacy-secp256k1")]
pub use keypair::KEYPAIR_ACCOUNT_KIND;
#[cfg(feature = "legacy-secp256k1")]
pub use legacy::LEGACY_ACCOUNT_KIND;
pub use mldsa::MLDSA_ACCOUNT_KIND;
#[cfg(feature = "legacy-secp256k1")]
pub use multisig::MULTISIG_ACCOUNT_KIND;
#[cfg(feature = "legacy-secp256k1")]
pub use resident::RESIDENT_ACCOUNT_KIND;
