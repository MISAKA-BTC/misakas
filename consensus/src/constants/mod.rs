// Re-exports constants from consensus core for internal crate usage.
// `perf` is named as `crate::constants::perf`; the glob is that path.
#[allow(unused_imports)]
pub use kaspa_consensus_core::config::constants::*;
pub(crate) use kaspa_consensus_core::constants::*;
