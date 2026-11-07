//! A parameter set whose pruning point actually moves, so IBD can be tested.
//!
//! A node only runs an IBD when it cannot resolve a peer's tip through orphan resolution, and the
//! block locator used for that check ends at the syncer's **pruning point**
//! (`create_block_locator_from_pruning_point`). On a chain that has never pruned, the pruning point
//! is genesis — which every node has — so a fresh node always finds a match, unorphans its way
//! forward, and never enters IBD. Measured, not assumed: at simnet parameters a follower 1500
//! blocks behind logs `Orphaned 19 ... Unorphaned 19` and syncs without an IBD.
//!
//! simnet's pruning depth is 1,080,000 blocks, so the honest way to reach the IBD path in a test is
//! to shorten it. That cannot be done by lowering `pruning_depth` alone: the depth's own lower
//! bound is dominated by `4 * mergeset_size_limit * ghostdag_k`, which at simnet's k=124 and
//! mergeset=248 is over 123,000 by itself. A coherent shallow preset has to bring k and the
//! mergeset limit down with it — which is what this is.
//!
//! It is a testing preset and nothing else. It is delivered through `--override-params-file`, the
//! supported mechanism for adjusted parameters, which refuses to run on mainnet.

use kaspa_consensus::params::{BlockrateParams, OverrideParams, SIMNET_PARAMS};
use std::path::PathBuf;

/// Blocks to mine before the leader's pruning point leaves genesis. Derived from the depths below
/// with headroom, since the pruning point advances in steps rather than continuously.
pub const BLOCKS_TO_PRUNE: usize = 3500;

/// GHOSTDAG k. Small because it enters the pruning-depth lower bound quadratically with the
/// mergeset limit. Safe here because the test mines serially, so every block has one parent and the
/// mergeset never approaches the limit.
const SHALLOW_GHOSTDAG_K: u16 = 4;
const SHALLOW_MERGESET_SIZE_LIMIT: u64 = 10;
const SHALLOW_MERGE_DEPTH: u64 = 100;
const SHALLOW_FINALITY_DEPTH: u64 = 300;
/// Above BOTH bounds that matter, and still reachable by mining.
///
/// The first is the depth's own formula, `finality + 2*merge + 4*mergeset*k + 2*k + 2` = 670. The
/// second is the DNS overlay's backward walk, and it is the one the first attempt at this preset
/// missed: `GENESIS_ACTIVE_DNS_PARAMS` (which simnet uses) has `unbonding_period_blocks = 700` and
/// `evidence_window_blocks = 300`, so the overlay reads acceptance data up to ~700 blocks back. A
/// pruning depth of 300 pruned block bodies the overlay still walked to, and the virtual processor
/// panicked with `KeyNotFound(BlockTransactions/...)`.
///
/// A parameter set is only coherent if the pruning depth contains every window anything else reads.
const SHALLOW_PRUNING_DEPTH: u64 = 2000;
/// The proof's per-level block requirement. simnet's 1000 cannot be met by a chain this short.
const SHALLOW_PRUNING_PROOF_M: u64 = 20;

/// The preset itself.
pub fn shallow_pruning_params() -> OverrideParams {
    let mut overrides = OverrideParams {
        blockrate: Some(BlockrateParams {
            // 1 block/s: the DAG stays a chain under serial mining, which is what keeps a small
            // ghostdag k honest.
            target_time_per_block: 1000,
            ghostdag_k: SHALLOW_GHOSTDAG_K,
            past_median_time_sample_rate: 1,
            difficulty_sample_rate: 1,
            max_block_parents: 10,
            mergeset_size_limit: SHALLOW_MERGESET_SIZE_LIMIT,
            merge_depth: SHALLOW_MERGE_DEPTH,
            finality_depth: SHALLOW_FINALITY_DEPTH,
            pruning_depth: SHALLOW_PRUNING_DEPTH,
            // Must stay well inside the pruning depth. Leaving simnet's 1000 here against a
            // pruning depth of 300 is what made the first attempt at this preset panic the virtual
            // processor: acceptance data still referenced coinbase blocks whose transactions had
            // already been pruned (`KeyNotFound(BlockTransactions/...)`). Every depth in a preset
            // has to shrink together or the set is not coherent.
            coinbase_maturity: 10,
        }),
        pruning_proof_m: Some(SHALLOW_PRUNING_PROOF_M),
        ..OverrideParams::from(SIMNET_PARAMS)
    };
    // `From<Params>` fills these from simnet; re-assert the ones the shallow depths depend on so a
    // future simnet change cannot silently make this preset inconsistent.
    overrides.pre_crescendo_target_time_per_block = Some(1000);
    overrides
}

/// Write the preset where `--override-params-file` can read it. Both daemons in a test must use the
/// same file, or they would disagree about consensus rules and — correctly — refuse to peer.
pub fn write_shallow_pruning_params(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("misaka-shallow-pruning-{}-{}.json", std::process::id(), tag));
    std::fs::write(&path, serde_json::to_string_pretty(&shallow_pruning_params()).unwrap()).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Emits the preset so it can be handed to real nodes via `--override-params-file`.
    ///
    /// The same values the in-process tests use, so a regression run on real hosts is testing the
    /// same rules rather than a hand-transcribed approximation of them.
    #[test]
    fn print_shallow_pruning_params_json() {
        println!("SHALLOW-PRESET-JSON-BEGIN");
        println!("{}", serde_json::to_string(&shallow_pruning_params()).unwrap());
        println!("SHALLOW-PRESET-JSON-END");
    }
}
