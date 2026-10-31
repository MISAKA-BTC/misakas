//! A5 deterministic credit reads and accounting plans for A0 contract revision 1.
//!
//! No native event, signature domain, activation height or persisted pass flag is
//! introduced. A4 owns verdicts; A6 owns rooted state and the atomic journal.
//! [`credit::UnavailableNativeCredit`] refuses native authorization until those
//! adapters exist. The in-memory fee book is a private integration reference,
//! not a coin issuer or a substitute for payer-signature/UTXO validation.

pub mod credit;
pub mod fees;
