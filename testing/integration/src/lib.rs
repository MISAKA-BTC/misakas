//! Integration tests for the PALW branch.
//!
//! # Running these
//!
//! ```text
//! MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-testing-integration --lib
//! ```
//!
//! PALW proof-of-work is active from genesis on devnet (ADR-0035), and validating one header there
//! replays a pinned 1.2 GB LLM. The variable selects the model-free fixture tag family instead —
//! the reason that family exists. It is honored on **devnet only**
//! (`kaspa_pow::palw::fixture_permitted_on`), so the simnet daemons in this suite are untouched by
//! it and still validate under their real rules. CI exports the same variable.
//!
//! ## Why that used to be three invocations
//!
//! The confinement to devnet lived in kaspad's startup rail as a process-wide `exit(1)`: a node on
//! any non-devnet network refused to start while the variable was set. This suite runs devnet
//! consensuses (which need it) and simnet daemons (which aborted on it), so no single value ran the
//! suite — with it, `rpc_tests`, `ibd_participation_tests` and `daemon_mining_test` aborted;
//! without it, `consensus_integration_tests` and `daemon_cleaning_test` did.
//!
//! Aborting was over-broad rather than wrong: simnet's `pow_palw_activation` is `never()`, so the
//! variable cannot affect a single tag there. The confinement now lives where the tag is computed,
//! keyed on the network id already passed to `palw_l1_tag`, and the rail warns instead of exiting
//! when PALW is inactive on the network it is starting.
//!
//! ## The measurement trap this left behind
//!
//! An abort prints neither `test result: FAILED` nor a panic line, so a check that greps a
//! workspace run for those reports a clean sweep over a suite that stopped partway. Two claims of
//! "workspace failure 0" were made that way before anyone noticed. `cargo nextest`, which CI uses,
//! gives each test its own process and so degrades an abort to one failed test rather than a
//! truncated run — worth knowing when reading either kind of output.

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
