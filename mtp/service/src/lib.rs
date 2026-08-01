//! MISAKA Testnet Points Program (MTP) — **service layer** (ADR-0038).
//!
//! The deterministic core ([`misaka_mtp`]) and the Sybil-aggregation collectors
//! ([`misaka_mtp_collectors`]) are the trust-critical, bit-reproducible half of
//! MTP. This crate is the **I/O ring** ADR-0027 deliberately excluded and
//! ADR-0038 designs: registration + nonce lifecycle, the single-attribution
//! resolver that closes namespace splitting (G1/I-MTP-1), the fresh-window epoch
//! cron (G3/I-MTP-3), ingestion-time caps and multiplier derivation
//! (G2/G4 → I-MTP-2/5/6), append-only signed-ledger publication with
//! supersede + finality horizon (D6/I-MTP-13), and the read-only self-serve
//! points query surface (D3).
//!
//! ## Boundary this crate keeps
//! Everything that decides a *point value* stays in [`misaka_mtp`] (pure,
//! integer-only, signed). This crate only decides *which facts are admissible*
//! (identity resolution, fact authenticity, caps) and *how the signed artifact
//! is published and served*. The trust-critical service-layer logic —
//! [`registry`], [`ingest`], [`epoch`], [`publish`] — is written as pure,
//! deterministic, unit-tested functions; the genuinely non-deterministic edge
//! (dialing peers, wRPC, the GitHub API, the wall clock, the RNG) is injected at
//! the [`collectors`] / [`main`] boundary so the fraud controls are all testable.
//!
//! ## Testnet-only (D1)
//! There is no mainnet mode: [`config::ServiceConfig`] pins the testnet address
//! prefix as a value the binary can only ever construct as testnet, and every
//! published ledger binds its `network` into the signed digest (core).

pub mod config;
pub mod epoch;
pub mod http;
pub mod ingest;
pub mod publish;
pub mod query;
pub mod registry;
pub mod store;

pub use config::{Role, ServiceConfig};
pub use epoch::{EpochError, build_epoch, build_epoch_ledger, resolve_attribution, run_epoch};
pub use http::{HttpState, serve as serve_http, serve_with_shutdown as serve_http_with_shutdown};
pub use ingest::{
    FixedActivity, LabelEvent, VersionObservation, cap_epoch_load_window, derive_fast_follow, derive_geo_diverse, gated_bug_event,
    resolve_c3_c4,
};
pub use publish::{ArchiveError, IndexEntry, LedgerArchive};
pub use query::{EpochView, PointsView, QueryError};
pub use registry::{AttributionError, Attributor, ClaimToken, NonceError, NonceStore, RegistrationRecord, claim_token, extract_claim_token};
pub use store::{PersistentStore, StoreError, Timed};

/// Service-layer domain-separation key for the node claim-token (I-MTP-11).
/// Disjoint from the five core `misaka-mtp-v1/*` ML-DSA contexts; used only as a
/// keyed-hash key (never an ML-DSA context), so a claim-token can never collide
/// with — or be replayed as — a registration, claim, or ledger signature.
pub const MTP_CLAIM_TOKEN_CONTEXT: &[u8] = b"misaka-mtp-v1/claim-token/blake2b";
