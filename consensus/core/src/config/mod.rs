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

use crate::palw_pruned_frontier::PalwPruningSnapshotCheckpoint;
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

    /// One-shot maintenance pass (`kaspad --reset-invalid-marks`): on startup, clear every locally
    /// persisted `StatusInvalid` mark so the affected blocks are re-requested and re-validated under
    /// the CURRENT rules. Recovers a node whose database was poisoned by an older binary that
    /// rejected — and permanently marked — blocks the current rules accept, which otherwise leaves
    /// IBD looping forever on `missing parents` / `invalid parents` for the same hashes.
    /// Node-local: statuses are not consensus state, so this cannot fork the node.
    pub reset_invalid_marks: bool,

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

    /// Operator-authenticated Header-v4 pruning boundaries. Node-local and consensus-neutral: these
    /// values authorize importing one exact canonical sidecar, but never change header/block validity
    /// or activate PALW on any network preset.
    pub palw_pruning_snapshot_checkpoints: Vec<PalwPruningSnapshotCheckpoint>,

    /// Governance allowlist of scheduler key fingerprints
    /// (`palw::search_snapshot::scheduler_key_id`) whose signed search assignments this node will
    /// admit. Node-local and consensus-neutral; EMPTY IS FAIL-CLOSED: with no allowlisted keys,
    /// assignment-resolved search snapshots are rejected and only zero-sentinel diagnostic
    /// snapshots (no mint path, no P2P) can be admitted.
    pub palw_search_scheduler_allowlist: Vec<kaspa_hashes::Hash64>,

    /// Node-local activation lever for chain-derived (permissionless) Header-v4 pruning-snapshot
    /// import. Default `false`; a shipped preset never sets it. When `false`, the
    /// `ChainDerivedHeaderBundle` provenance is not admitted regardless of what a peer advertises, so
    /// peer import stays fenced to the closed-network v3 and operator-pinned v4 paths. Setting it
    /// authorizes the pre-install, chain-derived authentication path (see
    /// `docs/adr-permissionless-snapshot-authentication.md`); it is StopShip until that wiring is
    /// complete and independently reviewed, and it does not activate PALW or change any commitment.
    pub palw_permissionless_snapshot_auth: bool,

    /// 2026-08-01 bystander-wedge proposal ② (defense-in-depth): while this node's own view of
    /// the selected chain is STALE (sink timestamp far behind wall clock — IBD, deep catch-up
    /// after downtime), do not LATCH newly confirmable DNS anchors into the reorg gate. A
    /// mid-sync view can be showing a branch the live network has already left, and a latched
    /// dead-branch anchor is exactly the wedge proposals ①/③ addressed. Node-local, NOT
    /// consensus-sensitive — the latch only steers this node's own reorg admission; it is never
    /// a block-validity input and never feeds a consensus derivation. Default `false` (test
    /// harnesses and sim tools drive chains with synthetic timestamps); `kaspad` sets it
    /// unconditionally in `Args::apply_to_config`.
    pub hold_dns_confirm_while_unsynced: bool,

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
    /// §12.3 v2: when to write a state-history ANCHOR and how many to keep.
    ///
    /// Replaces the `evm_number % EVM_CHECKPOINT_INTERVAL` rule, which reproduced the
    /// entire EVM state every 2048 EVM blocks — minutes on a 10 BPS chain — uncompressed
    /// and with bytecode inlined. Node-local and consensus-neutral: an anchor is
    /// reconstruction data, never a committed value.
    pub evm_checkpoint_policy: crate::evm::EvmCheckpointPolicy,
    /// §5.8: whether finalized EVM history is EXPORTED to cold segments at pruning
    /// advance, and where. When active, the pruning processor archives the range it
    /// is about to reclaim into immutable segment files BEFORE deleting the rows,
    /// and never deletes an EVM row the export has not yet covered (the interlock).
    /// L1 pruning itself is never delayed. `None` dir or `Off` mode ⇒ inert (the
    /// EVM-row delete floor stays at the pruning point, today's behaviour).
    pub evm_segment_export: crate::evm::EvmSegmentExport,
    pub evm_segment_dir: Option<std::path::PathBuf>,
    /// Per-segment EVM retention and the node role behind it.
    ///
    /// The half of the capacity story that is not about writing less: EVM data was
    /// reclaimed only by the L1 pruning processor, which correctly stands down while
    /// consensus is transitional — i.e. for the whole of IBD, the node's
    /// highest-write period. RPC-only segments retain on their own schedule.
    pub evm_retention_policy: crate::evm::EvmRetentionPolicy,
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
            reset_invalid_marks: false,
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
            palw_pruning_snapshot_checkpoints: vec![],
            palw_search_scheduler_allowlist: vec![],
            palw_permissionless_snapshot_auth: false,
            hold_dns_confirm_while_unsynced: false,
            evm_history_mode: crate::evm::EvmHistoryMode::Recent,
            evm_shadow_state_backend: false,
            evm_flat_authoritative: false,
            evm_retire_206: false,
            evm_prune_legacy_206: false,
            evm_checkpoint_policy: crate::evm::EvmCheckpointPolicy::default(),
            evm_retention_policy: crate::evm::EvmRetentionPolicy::default(),
            evm_segment_export: crate::evm::EvmSegmentExport::Off,
            evm_segment_dir: None,
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

    /// ADR-0042 StopShip lever: admit chain-derived (permissionless) Header-v4 pruning-snapshot
    /// import on THIS node.
    ///
    /// Deliberately shaped as an explicit, argument-less builder step rather than a
    /// `set_x(bool)` setter, so it cannot be enabled by threading a variable through a call site —
    /// turning it on is always a visible, greppable act. `Config::new` leaves the field `false` and
    /// NO preset touches it (see `no_preset_enables_palw_permissionless_snapshot_auth`); in-tree this
    /// is for tests only, and the sole production entry point is the fenced kaspad flag
    /// `--palw-permissionless-snapshot-auth`, which refuses to start on anything but a non-inert,
    /// structurally valid, archival Header-v4 network.
    ///
    /// The lever is node-local and consensus-neutral: it changes only which pruning boundaries this
    /// node is willing to IMPORT, never header/block validity and never a commitment.
    pub fn set_palw_permissionless_snapshot_auth(mut self) -> Self {
        self.config.palw_permissionless_snapshot_auth = true;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{
        DEVNET_PALW_PARAMS, DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, STAGING_MAINNET_PALW_PARAMS, TESTNET_PALW_PARAMS,
        TESTNET_PARAMS,
    };

    fn all_presets() -> [(&'static str, Params); 7] {
        [
            ("mainnet", MAINNET_PARAMS),
            ("testnet-10", TESTNET_PARAMS),
            ("testnet-palw-110", TESTNET_PALW_PARAMS),
            ("devnet-palw-111", DEVNET_PALW_PARAMS),
            ("staging-mainnet-palw", STAGING_MAINNET_PALW_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
        ]
    }

    /// ADR-0042 preset fence. The permissionless (chain-derived) snapshot-auth lever must be OFF on
    /// every shipped network, including the ONE non-inert Header-v4 preset — that preset is exactly
    /// where a default-on lever would be dangerous rather than inert, so it is asserted here and not
    /// only in the negative cases.
    ///
    /// This is the property that makes the whole change landable: with the lever `false`, the
    /// importer's AND-gate filters the chain-derived bundle to `None` and v3 / operator-pinned
    /// behaviour is byte-identical to the pre-ADR-0042 tree.
    #[test]
    fn no_preset_enables_palw_permissionless_snapshot_auth() {
        for (name, params) in all_presets() {
            assert!(
                !Config::new(params.clone()).palw_permissionless_snapshot_auth,
                "{name}: Config::new must leave the ADR-0042 lever off"
            );
            assert!(
                !ConfigBuilder::new(params.clone()).build().palw_permissionless_snapshot_auth,
                "{name}: the plain ConfigBuilder path must leave the ADR-0042 lever off"
            );
            // The neighbouring builder steps an operator/test is most likely to combine with it.
            assert!(
                !ConfigBuilder::new(params.clone())
                    .set_archival()
                    .enable_sanity_checks()
                    .skip_proof_of_work()
                    .build()
                    .palw_permissionless_snapshot_auth,
                "{name}: no other builder step may enable the ADR-0042 lever as a side effect"
            );
            assert!(
                !Config::new(params).to_builder().build().palw_permissionless_snapshot_auth,
                "{name}: to_builder round-trip must not invent the ADR-0042 lever"
            );
        }
    }

    /// The lever is reachable ONLY through its own explicit builder step, and a round-trip through
    /// `to_builder` preserves it (so a test that enabled it cannot silently lose it mid-setup).
    #[test]
    fn palw_permissionless_snapshot_auth_is_reachable_only_through_its_explicit_builder_step() {
        let config = ConfigBuilder::new(STAGING_MAINNET_PALW_PARAMS).set_palw_permissionless_snapshot_auth().build();
        assert!(config.palw_permissionless_snapshot_auth);
        assert!(config.to_builder().build().palw_permissionless_snapshot_auth);
        // Enabling it changes nothing else about the configuration.
        let baseline = ConfigBuilder::new(STAGING_MAINNET_PALW_PARAMS).build();
        assert_eq!(config.params.net, baseline.params.net);
        assert_eq!(config.is_archival, baseline.is_archival);
        assert_eq!(config.params.palw_algo4_accept, baseline.params.palw_algo4_accept);
        assert_eq!(config.palw_pruning_snapshot_checkpoints, baseline.palw_pruning_snapshot_checkpoints);
    }
}
