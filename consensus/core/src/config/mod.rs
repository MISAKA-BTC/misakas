pub mod bps;
pub mod constants;
pub mod genesis;
pub mod params;
pub mod premine;

use kaspa_utils::networking::{ContextualNetAddress, NetAddress};

#[cfg(feature = "devnet-prealloc")]
use crate::utxo::utxo_collection::UtxoCollection;
#[cfg(feature = "devnet-prealloc")]
use std::sync::Arc;

use std::ops::Deref;

use {
    constants::perf::{PERF_PARAMS, PerfParams},
    params::Params,
};

/// Various consensus configurations all bundled up under a single struct. Use `Config::new` for directly building from
/// a `Params` instance. For anything more complex it is recommended to use `ConfigBuilder`. NOTE: this struct can be
/// implicitly de-refed into `Params`
#[derive(Clone, Debug)]
pub struct Config {
    /// Consensus params
    pub params: Params,
    /// Performance params
    pub perf: PerfParams,

    //
    // Additional consensus configuration arguments which are not consensus sensitive
    //
    pub process_genesis: bool,

    /// Indicates whether this node is an archival node
    pub is_archival: bool,

    /// Enable various sanity checks which might be compute-intensive (mostly performed during pruning)
    pub enable_sanity_checks: bool,

    // TODO: move non-consensus parameters like utxoindex to a higher scoped Config
    /// Enable the UTXO index
    pub utxoindex: bool,

    /// Enable RPC commands which affect the state of the node
    pub unsafe_rpc: bool,

    /// Allow the node to accept blocks from RPC while not synced
    /// (required when initiating a new network from genesis)
    pub enable_unsynced_mining: bool,

    /// Allow mainnet mining. Until a stable Beta version we keep this option off by default
    pub enable_mainnet_mining: bool,

    pub user_agent_comments: Vec<String>,

    /// If undefined, sets it to 0.0.0.0
    pub p2p_listen_address: ContextualNetAddress,

    pub externalip: Option<NetAddress>,

    pub block_template_cache_lifetime: Option<u64>,

    #[cfg(feature = "devnet-prealloc")]
    pub initial_utxo_set: Arc<UtxoCollection>,

    pub disable_upnp: bool,

    /// A scale factor to apply to memory allocation bounds
    pub ram_scale: f64,

    /// The number of days to keep data for
    pub retention_period_days: Option<f64>,

    /// kaspa-pq EVM Lane (§12): this node's EVM state-history retention mode
    /// (`--evm-history-mode`). Node-local, NOT consensus-sensitive — it only
    /// controls whether the archive diff/checkpoint rows (prefixes 220/221) are
    /// written and how long they survive pruning; it never affects block validity
    /// or any commitment. `head` writes no diffs; `recent` keeps them to the
    /// pruning boundary; `archive` preserves EVM state history past pruning.
    pub evm_history_mode: crate::evm::EvmHistoryMode,

    /// C-01 state-backend (design v0.1, Stage 1, slice S4): node-local SHADOW
    /// dual-write of the flat latest-canonical state backend, with a per-block
    /// live differential against the committed snapshot. `false` by default and
    /// on every current network. A divergence HALTS the node (never serve a wrong
    /// root); the committed bytes never depend on the flat store, so toggling this
    /// is consensus-neutral — it only validates the backend before cutover.
    pub evm_shadow_state_backend: bool,

    /// kaspa-pq C-01 (slice S9): seed the EVM executor from the validated flat/reconstruct parent
    /// state (the cutover seed) instead of the per-block 206 snapshot. Effective only together with
    /// `evm_shadow_state_backend` (which maintains + validates the flat store). `false` by default
    /// and on every current network. The flat seed is asserted byte-identical to 206 BEFORE the
    /// executor uses it (HALT on divergence — never a false disqualification), and 206 is still
    /// written, so toggling this is consensus-neutral and reversible.
    pub evm_flat_authoritative: bool,

    /// kaspa-pq C-01 (slice S9b): STOP persisting the per-block 206 state snapshot. The flat backend
    /// — validated against the executor's in-memory post-state every block by the S4 write-side check
    /// (no dependency on 206) — becomes the sole persisted post-state; the executor seeds from it (S9)
    /// and reads (RPC / IBD pruning-point export) fall back to flat-materialize / §12-reconstruct.
    /// Effective only together with `evm_flat_authoritative`. `false` by default and on every current
    /// network. Node-local; toggling it changes only what THIS node persists/serves, never a
    /// commitment, so it is consensus-neutral. Requires `recent`/`archive` history (not `head`, which
    /// keeps no §12 history for the pruning-point export). REVERSIBILITY: to turn it back off, keep
    /// `evm_flat_authoritative` ON across the revert — blocks committed while retired have no 206, so
    /// the executor still seeds them from the flat store (their flat seed is reconstructed +
    /// root-validated). Disabling BOTH flags at once while retire-committed blocks are still unpruned
    /// would leave those parents with neither a 206 snapshot nor a flat seed (the verifier HALTs rather
    /// than fork); wait until the chain has advanced past them (they get pruned) before disabling
    /// `evm_flat_authoritative`.
    pub evm_retire_206: bool,

    /// kaspa-pq C-01 (slice S9b-prune): ONE-SHOT, IRREVERSIBLE bulk reclamation of the LEGACY per-block
    /// 206 state snapshot store that accumulated BEFORE `evm_retire_206` stopped writing it. The existing
    /// per-block pruner already reclaims 206 for blocks as they fall below the pruning point, so this only
    /// brings forward the reclamation of the rows still in the keep-window (and on archival nodes, all of
    /// them) instead of waiting for the pruning point to slide. Runs once at node startup, then is a no-op
    /// (the store is empty). EFFECTIVE ONLY when `evm_retire_206` is itself effective (i.e. together with
    /// `evm_flat_authoritative` + `evm_shadow_state_backend`) — otherwise refused with a warning, because
    /// deleting 206 while it is still the executor seed source would HALT the node. With those prerequisites
    /// the executor seeds from the flat/reconstruct parent and a present 206 is only a redundant byte-compare
    /// oracle, so the bulk delete leaves the seed itself unchanged (consensus-neutral, node-local). `false`
    /// by default and on every current network.
    pub evm_prune_legacy_206: bool,
}

impl Config {
    pub fn new(params: Params) -> Self {
        Self::with_perf(params, PERF_PARAMS)
    }

    pub fn with_perf(params: Params, perf: PerfParams) -> Self {
        Self {
            params,
            perf,
            process_genesis: true,
            is_archival: false,
            enable_sanity_checks: false,
            utxoindex: false,
            unsafe_rpc: false,
            enable_unsynced_mining: false,
            enable_mainnet_mining: false,
            user_agent_comments: Default::default(),
            externalip: None,
            p2p_listen_address: ContextualNetAddress::unspecified(),
            block_template_cache_lifetime: None,

            #[cfg(feature = "devnet-prealloc")]
            initial_utxo_set: Default::default(),
            disable_upnp: false,
            ram_scale: 1.0,
            retention_period_days: None,
            evm_history_mode: crate::evm::EvmHistoryMode::Recent,
            evm_shadow_state_backend: false,
            evm_flat_authoritative: false,
            evm_retire_206: false,
            evm_prune_legacy_206: false,
        }
    }

    pub fn to_builder(&self) -> ConfigBuilder {
        ConfigBuilder { config: self.clone() }
    }
}

impl AsRef<Params> for Config {
    fn as_ref(&self) -> &Params {
        &self.params
    }
}

impl Deref for Config {
    type Target = Params;

    fn deref(&self) -> &Self::Target {
        &self.params
    }
}

pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    pub fn new(params: Params) -> Self {
        Self { config: Config::new(params) }
    }

    pub fn set_perf_params(mut self, perf: PerfParams) -> Self {
        self.config.perf = perf;
        self
    }

    pub fn adjust_perf_params_to_consensus_params(mut self) -> Self {
        self.config.perf.adjust_to_consensus_params(&self.config.params);
        self
    }

    pub fn edit_consensus_params<F>(mut self, edit_func: F) -> Self
    where
        F: Fn(&mut Params),
    {
        edit_func(&mut self.config.params);
        self
    }

    pub fn apply_args<F>(mut self, edit_func: F) -> Self
    where
        F: Fn(&mut Config),
    {
        edit_func(&mut self.config);
        self
    }

    pub fn skip_proof_of_work(mut self) -> Self {
        self.config.params.skip_proof_of_work = true;
        self
    }

    pub fn set_archival(mut self) -> Self {
        self.config.is_archival = true;
        self
    }

    pub fn enable_sanity_checks(mut self) -> Self {
        self.config.enable_sanity_checks = true;
        self
    }

    pub fn skip_adding_genesis(mut self) -> Self {
        self.config.process_genesis = false;
        self
    }

    pub fn build(self) -> Config {
        self.config
    }
}
