pub mod block_depth;
pub mod coinbase;
pub mod difficulty;
// kaspa-pq Selected-Parent EVM Lane (ADR-0020): the consensus → `kaspa-evm`
// executor seam. Re-exports the executor only under the `evm` feature.
pub mod evm;
pub mod ghostdag;
pub mod parents_builder;
pub mod past_median_time;
pub mod pruning;
pub mod pruning_proof;
pub mod reachability;
pub mod relations;
pub mod sync;
pub mod transaction_validator;
pub mod traversal_manager;
pub(crate) mod utils;
pub mod window;
