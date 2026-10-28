//!
//! PSKT is a crate for working with Partially Signed Kaspa Transactions (PSKTs).
//! This crate provides following primitives: `PSKT`, `PSKTBuilder` and `Bundle`.
//! The `Bundle` struct is used for PSKT exchange payload serialization and carries
//! multiple `PSKT` instances allowing for exchange of Kaspa sweep transactions.
//!
//! kaspa-pq PQ-only (ADR-0019 §14): every PSKT primitive is keyed on
//! `secp256k1::PublicKey`/`Signature` (ECDSA/Schnorr/BIP32) — there is no ML-DSA
//! PSKT. The whole crate is therefore gated behind `legacy-secp256k1`; the
//! PQ-only wallet build links no secp256k1 and simply does not expose PSKT/PSKB.

#[cfg(feature = "legacy-secp256k1")]
pub mod bundle;
#[cfg(feature = "legacy-secp256k1")]
pub mod error;
#[cfg(feature = "legacy-secp256k1")]
pub mod global;
#[cfg(feature = "legacy-secp256k1")]
pub mod input;
#[cfg(feature = "legacy-secp256k1")]
pub mod output;
#[cfg(feature = "legacy-secp256k1")]
pub mod pskt;
#[cfg(feature = "legacy-secp256k1")]
pub mod role;
#[cfg(feature = "legacy-secp256k1")]
pub mod wasm;

#[cfg(feature = "legacy-secp256k1")]
mod convert;
#[cfg(feature = "legacy-secp256k1")]
mod utils;

#[cfg(feature = "legacy-secp256k1")]
pub mod prelude {
    pub use crate::bundle::Bundle;
    pub use crate::bundle::*;
    pub use crate::global::Global;
    pub use crate::input::Input;
    pub use crate::output::Output;
    pub use crate::pskt::*;

    // not quite sure why it warns of unused imports,
    // perhaps due to the fact that enums have no variants?
    #[allow(unused_imports)]
    pub use crate::role::*;
}
