//! Consensus → EVM executor seam (ADR-0020 §"PQ-only reconciliation").
//!
//! The lazy chain-context validation hook (P3) calls into the `kaspa-evm`
//! executor through this module. It is gated behind the non-default `evm` cargo
//! feature, so the default node never links revm/secp — the secp-free guarantee
//! enforced by `scripts/pq-ci-guard.sh` (`cargo tree -e normal`) is unaffected.
//!
//! Non-`evm` builds: this module re-exports nothing. The EVM lane is
//! `u64::MAX`-inert on every default network, so the executor is unreachable; a
//! node that somehow reaches EVM execution without the `evm` feature is
//! misconfigured and the P3 hook must fail loudly rather than silently fork.

#[cfg(feature = "evm")]
pub use kaspa_evm::{execute_block_evm, EvmBlockInput};
