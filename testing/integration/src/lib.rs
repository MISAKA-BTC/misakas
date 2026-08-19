//! Integration tests for the PALW branch.
//!
//! # Running these: two invocations, not one — and this is a branch condition, not a preference
//!
//! Since ADR-0035 (`e4848d2b`, 2026-08-17) devnet validates PALW LLM proof-of-work, so a test that
//! builds a devnet consensus needs either the pinned worker (`PALW_WORKER` + `MISAKA_PALW_GGUF`) or
//! `MISAKA_PALW_POW_FIXTURE=1`. The same release refuses that variable on any other network —
//! fixture tags are a different rule set, and honouring them on simnet would fork the node at the
//! first block — and the refusal ABORTS the process rather than ignoring the variable.
//!
//! The suite contains both kinds, so no single value of that variable runs it:
//!
//! ```text
//! MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-testing-integration --lib -- \
//!     --skip daemon_integration_tests --skip ibd_participation_tests --skip rpc_tests \
//!     --skip daemon_mining_test
//! cargo test -p kaspa-testing-integration --lib -- \
//!     ibd_participation_tests rpc_tests daemon_integration_tests::daemon_mining_test
//! MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-testing-integration --lib -- \
//!     daemon_integration_tests::daemon_cleaning_test
//! ```
//!
//! Measured 2026-08-19: every test passes under the setting its own network requires. Run as one
//! invocation the binary aborts (SIGABRT or SIGSEGV depending on which side is hit first), and an
//! abort prints neither `test result: FAILED` nor a panic line — so a check that greps for those
//! reports a clean run over a suite that never finished. `cargo nextest`, which CI uses, gives each
//! test its own process and so degrades to per-test failures instead of one abort; it still fails
//! the devnet tests, because CI sets no fixture variable.

#[cfg(feature = "heap")]
#[global_allocator]
#[cfg(not(feature = "heap"))]
static ALLOC: dhat::Alloc = dhat::Alloc;

pub mod common;
pub mod tasks;

#[cfg(test)]
pub mod consensus_integration_tests;

#[cfg(test)]
pub mod consensus_pipeline_tests;

#[cfg(test)]
pub mod daemon_integration_tests;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod mempool_benchmarks;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod subscribe_benchmarks;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod rpc_perf_benchmarks;

#[cfg(test)]
pub mod ibd_participation_tests;

#[cfg(test)]
pub mod rpc_tests;
