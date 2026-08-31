use clap::{Arg, ArgAction, Command, arg};
use kaspa_consensus_core::config::trusted_checkpoint::TrustedCheckpoint;
use kaspa_consensus_core::{
    config::Config,
    evm::EvmHistoryMode,
    network::{NetworkId, NetworkType},
};
use kaspa_core::kaspad_env::version;
use kaspa_notify::address::tracker::Tracker;
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_wrpc_server::address::WrpcNetAddress;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use std::{ffi::OsString, fs};
use toml::from_str;

#[cfg(feature = "devnet-prealloc")]
use kaspa_addresses::Address;
#[cfg(feature = "devnet-prealloc")]
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
#[cfg(feature = "devnet-prealloc")]
use kaspa_txscript::pay_to_address_script;
#[cfg(feature = "devnet-prealloc")]
use std::sync::Arc;

/// Operational role profile for constrained MISAKA nodes. A profile never changes
/// consensus rules; it only applies resource defaults for unspecified knobs and
/// refuses obviously incompatible runtime roles at startup. `Full` is the
/// historical no-op default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeProfile {
    /// No constraints, no resource overrides.
    #[default]
    Full,
    /// Permanent sync-only source: pruned consensus + P2P, no archive/index/validator/RPC.
    BootstrapPruned,
    /// One-shot fresh-DB catch-up from a `--connect` seed, then promote to bootstrap.
    RecoverySync,
    /// Staking/attestation node label; does not force `--enable-validator`.
    Validator,
    /// Archival node label; does not force `--archival`.
    Archive,
    /// Public RPC node label; does not force any RPC listener.
    PublicRpc,
}

impl NodeProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeProfile::Full => "full",
            NodeProfile::BootstrapPruned => "bootstrap-pruned",
            NodeProfile::RecoverySync => "recovery-sync",
            NodeProfile::Validator => "validator",
            NodeProfile::Archive => "archive",
            NodeProfile::PublicRpc => "public-rpc",
        }
    }

    pub const VARIANTS: [&'static str; 6] = ["full", "bootstrap-pruned", "recovery-sync", "validator", "archive", "public-rpc"];

    fn from_cli(s: &str) -> Option<Self> {
        Some(match s {
            "full" => NodeProfile::Full,
            "bootstrap-pruned" => NodeProfile::BootstrapPruned,
            "recovery-sync" => NodeProfile::RecoverySync,
            "validator" => NodeProfile::Validator,
            "archive" => NodeProfile::Archive,
            "public-rpc" => NodeProfile::PublicRpc,
            _ => return None,
        })
    }

    pub fn is_sync_only(&self) -> bool {
        matches!(self, NodeProfile::BootstrapPruned | NodeProfile::RecoverySync)
    }
}

impl std::fmt::Display for NodeProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

const VPS_8GB_RAM_SCALE: f64 = 0.3;
const VPS_8GB_ASYNC_THREADS: usize = 2;
const VPS_8GB_OUTPEERS: usize = 4;
const VPS_8GB_MAXINPEERS: usize = 32;
const VPS_8GB_RPCMAXCLIENTS: usize = 8;
const VPS_8GB_MIN_DISK_FREE_PERCENT: u8 = 15;
/// `--vps-8gb` warns when total system memory is below this value.
pub const VPS_8GB_MIN_SYSTEM_MEMORY_BYTES: u64 = 7_500_000_000;

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Args {
    // NOTE: it is best if property names match config file fields
    pub appdir: Option<String>,
    pub logdir: Option<String>,
    #[serde(rename = "nologfiles")]
    pub no_log_files: bool,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub rpclisten: Option<ContextualNetAddress>,
    /// kaspa-pq EVM Lane (ADR-0020 §16): interface:port for the Ethereum JSON-RPC
    /// adapter (effective only in an `--features evm` build).
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub evm_rpc_listen: Option<ContextualNetAddress>,
    /// kaspa-pq EVM Lane (§12 archive): EVM state-history retention mode
    /// (`head`/`recent`/`archive`). Default `recent`. Effective only in an
    /// `--features evm` build; the diff/checkpoint retention enforcement lands with
    /// the §12 archive writer.
    #[serde(default)]
    pub evm_history_mode: EvmHistoryMode,
    /// C-01 slice S4: node-local SHADOW dual-write of the flat state backend +
    /// per-block live differential vs the committed snapshot. Off by default;
    /// consensus-neutral (a divergence only halts this node, never forks).
    #[serde(default)]
    pub evm_shadow_state_backend: bool,
    /// C-01 S9: seed the EVM executor from the validated flat/reconstruct parent state instead of
    /// the 206 snapshot (the cutover seed). Requires `evm_shadow_state_backend`. Off by default;
    /// node-local + consensus-neutral (the seed is validated == 206 before use; 206 is still written).
    #[serde(default)]
    pub evm_flat_authoritative: bool,
    /// C-01 S9b: STOP persisting the per-block 206 state snapshot (the storage win — 206 stores a full
    /// state copy per kept block). The flat backend, already validated against the executor's in-memory
    /// post-state every block by the S4 write-side check, becomes the sole persisted post-state; reads
    /// (RPC / IBD pruning-point export) fall back to flat-materialize / §12-reconstruct. Requires
    /// `evm_flat_authoritative`. Off by default; node-local. Use `recent`/`archive` history (NOT `head`,
    /// which keeps no §12 history for the pruning-point export / historical reads).
    #[serde(default)]
    pub evm_retire_206: bool,
    /// C-01 S9b-prune: ONE-SHOT, IRREVERSIBLE bulk reclamation of the legacy per-block 206 EVM state
    /// snapshot store that accumulated before `--evm-retire-206`. Runs once at startup, then a no-op.
    /// Effective only when `--evm-retire-206` is itself effective (requires `--evm-flat-authoritative`
    /// + `--evm-shadow-state-backend`); otherwise refused with a warning. Off by default; node-local.
    #[serde(default)]
    pub evm_prune_legacy_206: bool,
    /// F2c (t10 recovery): ONE-SHOT startup backfill of the pruning point's EVM state anchor by
    /// reverse-replaying the retained §12 diffs from the flat head down to the pruning point.
    /// Verified against the pp's committed state_root before anything is persisted; idempotent.
    /// For retired-206 datadirs that predate the pruning processor's pp-anchor step. Off by default.
    #[serde(default)]
    pub evm_materialize_pp_anchor: bool,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub rpclisten_borsh: Option<WrpcNetAddress>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub rpclisten_json: Option<WrpcNetAddress>,
    #[serde(rename = "unsaferpc")]
    pub unsafe_rpc: bool,
    pub wrpc_verbose: bool,
    #[serde(rename = "loglevel")]
    pub log_level: String,
    pub async_threads: usize,
    #[serde(rename = "connect")]
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub connect_peers: Vec<ContextualNetAddress>,
    #[serde(rename = "addpeer")]
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub add_peers: Vec<ContextualNetAddress>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub listen: Option<ContextualNetAddress>,
    #[serde(rename = "uacomment")]
    pub user_agent_comments: Vec<String>,
    pub utxoindex: bool,
    pub reset_db: bool,
    #[serde(rename = "outpeers")]
    pub outbound_target: usize,
    #[serde(rename = "maxinpeers")]
    pub inbound_limit: usize,
    #[serde(rename = "rpcmaxclients")]
    pub rpc_max_clients: usize,
    pub max_tracked_addresses: usize,
    pub enable_unsynced_mining: bool,
    pub enable_mainnet_mining: bool,

    /// `<daa-score>:<block-hash>:<consensus-params-id>` — the history this operator vouches for.
    ///
    /// A hard constraint on which chains this node may adopt, not a preference. Unset means the
    /// node has no trust root beyond accumulated work, which is the weak-subjectivity gap
    /// ADR-0009 documents rather than a safe default.
    pub trusted_checkpoint: Option<String>,

    /// Enforce the chain-participation gate on a network where it is off by default.
    ///
    /// The gate is scoped to mainnet/testnet because a peerless devnet or simnet node has no
    /// competing branch to overlook. That is a default, not a law: a devnet with real peers wants
    /// production behaviour, and a test that means to exercise the gate has to be able to turn it
    /// on. Never needed on mainnet or testnet, where it is always enforced.
    pub enforce_chain_participation: bool,
    /// ADR-0025's "until an operator intervenes", as a startup flag: clear a PERSISTED
    /// `Quarantined` participation state once at boot, loudly. `Quarantined` never clears on its
    /// own by design, and without this the only exit was deleting the meta-DB key by hand — which
    /// the 2026-08-10 recovery had to contemplate on two of three fleet nodes at once. Clears
    /// quarantine ONLY (a pending CandidateReview keeps its deadline), and fires on every boot it
    /// is present for: remove it from the unit after the node is back.
    pub clear_quarantine: bool,

    // kaspa-pq Phase 11 (ADR-0010): in-process DNS-overlay validator service. Default off.
    pub enable_validator: bool,
    pub validator_key: Option<String>,
    /// ADR-0042: run the in-process PALW-RC block producer. Only a `ConsensusV2` network has
    /// anything for it to do, and it says so and stops otherwise.
    pub palw_produce: bool,
    /// ADR-0060 Decision 1: run the bondless heartbeat miner, paying the lane's fees to this
    /// ML-DSA-87 address. One CPU thread, fee-only, only meaningful on a ConsensusV2 network.
    pub palw_heartbeat_miner_address: Option<String>,
    pub palw_producer_key: Option<String>,
    pub palw_producer_bond: Option<String>,
    /// **Artifact files for classes whose weights cannot be derived** (repeatable).
    ///
    /// The floor is minted from a pinned seed on every node, so an RC producer needs none of
    /// these. A converted class is somebody's checkpoint quantized offline — nothing the node
    /// holds can re-derive it — so the bytes travel as a file. Each is digest-checked on load and
    /// then matched against what the CHAIN says the class is; a file matching neither the
    /// registered graph nor the registered weights is not used.
    pub palw_class_artifact: Vec<String>,
    /// **The byte bound on artifacts this node holds resident** (0 = unbounded).
    ///
    /// ADR-0067 makes the class registry permissionless, which multiplies MODELS — and a node that
    /// answered every registration by holding its weights would have its disk chosen by strangers.
    /// Registration obligates no node to hold anything. This is the number that says so: artifacts
    /// load in the order given, and loading stops here. What is skipped is named in the log, and a
    /// class this node does not hold is one it never declares and is never drawn to judge.
    pub palw_class_cache_bytes: u64,
    /// **Register the class of this node's converted artifact on the running chain, once.**
    ///
    /// A network is born with the classes its ruleset id commits to; every later one arrives as a
    /// signed `ClassRegistered` that carries its own profile (ADR-0049 Decision H). Nothing built
    /// or carried such an object, so gaining a class meant re-minting the network.
    /// `Some("")` means "register whatever single class my artifact matches"; a non-empty value
    /// names the model id when the artifact's shape matches several ledger siblings (the A16
    /// family shares one converted shape, so shape alone cannot pick between them).
    pub palw_register_class: Option<String>,
    pub palw_register_bond: bool,
    pub palw_dump_classes: bool,
    /// ADR-0067: arm the chain-registered-class arm (the fence's node half).
    pub palw_chain_classes: bool,
    /// ADR-0067 Decision 6: `class-id:file` pairs whose declarations this node should adopt.
    pub palw_class_carriage: Vec<String>,
    pub palw_bond_collateral: Option<u64>,
    /// **Produce for this class instead of the network's floor.**
    ///
    /// 128-hex. Defaults to `bundle.base_class_id`, which is the class every node can run and
    /// therefore the one that keeps the chain alive. A node that holds a worker for some other
    /// registered class names it here; one that names a class it cannot resolve a backend for is
    /// refused by `PalwBackendRegistry::resolve` rather than silently falling back, because a
    /// producer that quietly mined the floor instead would look like it was doing what it was told.
    pub palw_producer_class: Option<String>,
    /// Re-run every licensed claim and open a court against the ones this node cannot reproduce.
    pub palw_challenge: bool,
    /// DRILL ONLY: corrupt one lane of this leaf in every block this node produces.
    pub palw_drill_tamper_leaf: Option<u64>,
    /// DRILL ONLY: open a court against every licensed claim, reproduced or not.
    pub palw_drill_challenge_all: bool,
    pub palw_producer_pay_address: Option<String>,
    pub palw_panel: bool,
    pub palw_fee_outpoint: Option<String>,
    /// kaspa-pq EVM Lane v0.4 (§8.2/§16): the miner's EVM coinbase (20-byte hex,
    /// optional 0x) — claims the priority fees of this node's own payload txs.
    pub evm_fee_recipient: Option<String>,
    pub stake_bond: Option<String>,
    pub validator_mode: Option<String>,

    // MISAKA Verified LLM Token-Weighted BFT: the compute role. Default off, and additionally
    // inert on any network whose model cost table is empty (which is every shipped preset).
    pub enable_compute: bool,
    pub compute_worker: Option<String>,
    /// PALW v2 (Land stage): path to a `palw-agent` Unix socket to monitor. Observation only —
    /// health-probed and logged, feeding the capability handle nothing consensus-visible
    /// consumes yet. The VLT compute role (v1) is untouched by it. Served on Unix hosts only
    /// (see `crate::palw_agent`); elsewhere it warns and the capability stays withdrawn.
    pub compute_endpoint: Option<String>,
    pub compute_work_dir: Option<String>,
    pub compute_prompt: Option<String>,
    pub compute_max_tokens: Option<u32>,
    pub compute_timeout_secs: Option<u64>,
    pub compute_auto_challenge: bool,
    /// MISAKA devnet fixture: originate at most this many jobs, ever (persisted across restarts).
    pub compute_fixture_job_limit: Option<u32>,

    // MISAKA VLT activation, for PRIVATE devnets only. These are consensus fences: on a public
    // network they belong to a release, not to whoever started the node, so `apply_to_config`
    // refuses them anywhere but devnet/simnet.
    pub vlt_devnet_shadow_daa: Option<u64>,
    pub vlt_devnet_credit_window_epochs: u32,
    pub vlt_shadow_only: bool,
    /// MISAKA VLT PR 3: with `--vlt-devnet`, pin `credit_decay_bps` to 10_000 (a flat `d_τ = 1`).
    /// The 8/5/3/2/2 weight-plan devnet needs job counts to map to weights EXACTLY — under decay,
    /// validators that finish their quotas across different epochs drift off the plan's ratios.
    pub vlt_devnet_flat_decay: bool,

    // MISAKA Compute Token Program devnet fences + fixture ops, PRIVATE devnets only —
    // same refusal rule as the VLT fences above.
    /// `--tkn-devnet=<active_daa>`: open the TOK ledger fold + emission at this DAA, with the
    /// shadow fence `tkn_devnet_shadow_span` below it.
    pub tkn_devnet_active_daa: Option<u64>,
    pub tkn_devnet_shadow_span: u64,
    /// Flat per-epoch emission budget in whole TOK (atomic = ×10^8). Devnet-only calibration.
    pub tkn_devnet_epoch_budget_tok: u64,
    /// Fixture token ops, submitted by the validator service once the chain reaches each op's
    /// DAA: `to_hex128:amount_atomic:nonce:at_daa` per entry.
    pub tkn_fixture_transfers: Vec<String>,
    /// `amount_atomic:nonce:at_daa` per entry.
    pub tkn_fixture_burns: Vec<String>,

    pub testnet: bool,
    #[serde(rename = "netsuffix")]
    pub testnet_suffix: u32,
    pub devnet: bool,
    pub simnet: bool,
    pub archival: bool,
    pub sanity: bool,
    pub yes: bool,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub externalip: Option<ContextualNetAddress>,
    pub perf_metrics: bool,
    pub perf_metrics_interval_sec: u64,
    pub block_template_cache_lifetime: Option<u64>,

    #[cfg(feature = "devnet-prealloc")]
    pub num_prealloc_utxos: Option<u64>,
    #[cfg(feature = "devnet-prealloc")]
    pub prealloc_address: Option<String>,
    #[cfg(feature = "devnet-prealloc")]
    pub prealloc_amount: u64,

    pub disable_upnp: bool,
    #[serde(rename = "nodnsseed")]
    pub disable_dns_seeding: bool,
    #[serde(rename = "nogrpc")]
    pub disable_grpc: bool,
    pub ram_scale: f64,
    pub retention_period_days: Option<f64>,

    pub override_params_file: Option<String>,

    pub rocksdb_preset: Option<String>,
    pub rocksdb_wal_dir: Option<String>,
    pub rocksdb_cache_size: Option<usize>,

    /// Operational role profile for constrained VPS deployments. Sync-only profiles apply
    /// 8GB resource defaults and reject archive/index/validator/EVM-RPC roles.
    pub node_profile: NodeProfile,
    /// Convenience flag that applies the same 8GB resource defaults for unspecified knobs,
    /// regardless of the chosen node profile.
    #[serde(rename = "vps-8gb")]
    pub vps_8gb: bool,
    /// Refuse startup when the data mount has less than this percentage of free disk.
    /// `0` disables the gate; sync-only profiles and `--vps-8gb` default to 15.
    pub min_disk_free_percent: u8,

    /// Node RPC profile (design §9): a named bundle that enables a sensible set of
    /// RPC listeners so operators don't wire each one by hand. Explicit `--rpclisten*`
    /// / `--evm-rpc-listen` flags always win over the profile's defaults.
    pub profile: Option<String>,

    /// Acknowledge binding the node RPC listeners (gRPC / wRPC Borsh / wRPC JSON) to a
    /// NON-loopback address (design §7.1/§15.5). Without it, a public RPC bind still
    /// works but logs a security warning at startup (it is not a fail-closed refusal —
    /// that would break existing public deployments). The EVM RPC keeps its own
    /// fail-closed `MISAKA_ALLOW_PUBLIC_EVM_RPC` gate.
    pub allow_public_rpc: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            appdir: None,
            no_log_files: false,
            rpclisten_borsh: None,
            rpclisten_json: None,
            unsafe_rpc: false,
            async_threads: num_cpus::get(),
            utxoindex: false,
            reset_db: false,
            outbound_target: 8,
            inbound_limit: 128,
            rpc_max_clients: 128,
            max_tracked_addresses: 0,
            enable_unsynced_mining: false,
            enable_mainnet_mining: true,
            trusted_checkpoint: None,
            enforce_chain_participation: false,
            clear_quarantine: false,
            enable_validator: false,
            validator_key: None,
            palw_produce: false,
            palw_heartbeat_miner_address: None,
            palw_producer_key: None,
            palw_producer_bond: None,
            palw_class_artifact: Vec::new(),
            palw_class_cache_bytes: 0,
            palw_register_class: None,
            palw_register_bond: false,
            palw_dump_classes: false,
            palw_chain_classes: false,
            palw_class_carriage: Vec::new(),
            palw_bond_collateral: None,
            palw_producer_class: None,
            palw_challenge: false,
            palw_drill_tamper_leaf: None,
            palw_drill_challenge_all: false,
            palw_producer_pay_address: None,
            palw_panel: false,
            palw_fee_outpoint: None,
            evm_fee_recipient: None,
            stake_bond: None,
            validator_mode: None,
            enable_compute: false,
            compute_worker: None,
            compute_endpoint: None,
            compute_work_dir: None,
            compute_prompt: None,
            compute_max_tokens: None,
            compute_timeout_secs: None,
            compute_auto_challenge: false,
            compute_fixture_job_limit: None,
            vlt_devnet_shadow_daa: None,
            vlt_devnet_credit_window_epochs: 8,
            vlt_shadow_only: false,
            vlt_devnet_flat_decay: false,
            tkn_devnet_active_daa: None,
            tkn_devnet_shadow_span: 300,
            tkn_devnet_epoch_budget_tok: 1_000,
            tkn_fixture_transfers: Vec::new(),
            tkn_fixture_burns: Vec::new(),
            testnet: false,
            testnet_suffix: 10,
            devnet: false,
            simnet: false,
            archival: false,
            sanity: false,
            logdir: None,
            rpclisten: None,
            evm_rpc_listen: None,
            evm_history_mode: EvmHistoryMode::Recent,
            evm_shadow_state_backend: false,
            evm_flat_authoritative: false,
            evm_retire_206: false,
            evm_prune_legacy_206: false,
            evm_materialize_pp_anchor: false,
            wrpc_verbose: false,
            log_level: "INFO".into(),
            connect_peers: vec![],
            add_peers: vec![],
            listen: None,
            user_agent_comments: vec![],
            yes: false,
            perf_metrics: false,
            perf_metrics_interval_sec: 10,
            externalip: None,
            block_template_cache_lifetime: None,

            #[cfg(feature = "devnet-prealloc")]
            num_prealloc_utxos: None,
            #[cfg(feature = "devnet-prealloc")]
            prealloc_address: None,
            #[cfg(feature = "devnet-prealloc")]
            prealloc_amount: 10_000_000_000,

            disable_upnp: false,
            disable_dns_seeding: false,
            disable_grpc: false,
            ram_scale: 1.0,
            retention_period_days: None,
            override_params_file: None,
            rocksdb_preset: None,
            rocksdb_wal_dir: None,
            rocksdb_cache_size: None,
            node_profile: NodeProfile::Full,
            vps_8gb: false,
            min_disk_free_percent: 0,
            profile: None,
            allow_public_rpc: false,
        }
    }
}

impl Args {
    pub fn apply_to_config(&self, config: &mut Config) {
        config.utxoindex = self.utxoindex;
        config.disable_upnp = self.disable_upnp;
        config.unsafe_rpc = self.unsafe_rpc;
        config.enable_unsynced_mining = self.enable_unsynced_mining;
        config.enable_mainnet_mining = self.enable_mainnet_mining;
        config.is_archival = self.archival;
        // TODO: change to `config.enable_sanity_checks = self.sanity` when we reach stable versions
        config.enable_sanity_checks = true;
        config.user_agent_comments.clone_from(&self.user_agent_comments);
        config.block_template_cache_lifetime = self.block_template_cache_lifetime;
        config.p2p_listen_address = self.listen.unwrap_or(ContextualNetAddress::unspecified());
        config.externalip = self.externalip.map(|v| v.normalize(config.default_p2p_port()));
        config.ram_scale = self.ram_scale;
        config.retention_period_days = self.retention_period_days;
        config.evm_history_mode = self.evm_history_mode; // §12: EVM state-history retention
        config.evm_shadow_state_backend = self.evm_shadow_state_backend; // C-01 S4: shadow dual-write
        config.evm_flat_authoritative = self.evm_flat_authoritative; // C-01 S9: flat-authoritative executor seed
        config.evm_retire_206 = self.evm_retire_206; // C-01 S9b: stop persisting the per-block 206 snapshot
        config.evm_prune_legacy_206 = self.evm_prune_legacy_206; // C-01 S9b-prune: one-shot bulk reclamation of legacy 206
        config.evm_materialize_pp_anchor = self.evm_materialize_pp_anchor; // F2c: one-shot pp EVM anchor backfill

        // MISAKA VLT: private-devnet activation. Refused anywhere else — these are consensus
        // fences, and a node that moved them by flag would simply fork itself off the network it
        // thinks it is on. On a public network they belong to a release.
        if let Some(shadow_daa) = self.vlt_devnet_shadow_daa {
            let net = self.network().network_type();
            if !matches!(net, NetworkType::Devnet | NetworkType::Simnet) {
                panic!(
                    "--vlt-devnet is devnet/simnet only (got {net:?}). Moving a VLT activation fence is a consensus change \
                     and must ship in a release, not a command line."
                );
            }
            // The genesis hash goes in because the devnet fixture profile is derived from it: the
            // fixture of one network is not the fixture of another, so a fixture certificate is
            // meaningless anywhere but the devnet it was built for — a constraint that holds even
            // if the feature flag were somehow on in the wrong build.
            let genesis_hash = config.params.genesis.hash;
            let mut dns = config
                .params
                .dns_params
                .take()
                .expect("devnet/simnet ship with the DNS overlay configured")
                .with_vlt_devnet(shadow_daa, self.vlt_devnet_credit_window_epochs, self.vlt_shadow_only, genesis_hash);
            // Flat decay is a devnet CALIBRATION, not a rule change: `is_coherent` admits
            // `credit_decay_bps == 10_000` (d_τ = 1 for every τ). It exists so a job-quota plan
            // like 8/5/3/2/2 lands as exactly 400/250/150/100/100 VLT of weight regardless of
            // which epoch each validator finished in.
            if self.vlt_devnet_flat_decay {
                dns.vlt.credit_decay_bps = 10_000;
            }
            // Fail loudly rather than let the node start into a configuration `update_dns_state`
            // would silently refuse to leave Bootstrap for.
            assert!(
                dns.vlt_params_consistent(),
                "--vlt-devnet produced an inconsistent VLT configuration (credit window {}, K {}); \
                 the overlay would never leave Bootstrap",
                dns.vlt_credit_window_blue_score,
                self.vlt_devnet_credit_window_epochs
            );
            config.params.dns_params = Some(dns);
        } else if self.vlt_shadow_only {
            panic!("--vlt-shadow-only only means something together with --vlt-devnet");
        } else if self.vlt_devnet_flat_decay {
            panic!("--vlt-devnet-flat-decay only means something together with --vlt-devnet");
        }

        // MISAKA Compute Token Program devnet fences — same refusal rules as the VLT block above:
        // private networks only, and only on top of a running compute overlay.
        if let Some(active_daa) = self.tkn_devnet_active_daa {
            let net = self.network().network_type();
            if !matches!(net, NetworkType::Devnet | NetworkType::Simnet) {
                panic!(
                    "--tkn-devnet is devnet/simnet only (got {net:?}). Moving a token activation fence is a consensus \
                     change and must ship in a release, not a command line."
                );
            }
            if self.vlt_devnet_shadow_daa.is_none() {
                panic!(
                    "--tkn-devnet requires --vlt-devnet: emission settles over VLT credits, and a token program on an \
                     inert compute overlay is undefined (design v0.1 §10)."
                );
            }
            let dns = config.params.dns_params.take().expect("devnet/simnet ship with the DNS overlay configured").with_tkn_devnet(
                active_daa,
                self.tkn_devnet_shadow_span,
                self.tkn_devnet_epoch_budget_tok as u128 * 100_000_000,
            );
            // Fail loudly rather than start a node whose fold or settlement would silently refuse
            // to run — the devnet symptom would be "no [token] line, ever", which reads as a bug.
            assert!(
                dns.tkn_params_consistent(),
                "--tkn-devnet={active_daa} produced an inconsistent token configuration against the VLT fences \
                 (vlt_shadow={}, D_settle={}); raise --tkn-devnet above the VLT shadow fence",
                dns.vlt.vlt_shadow_activation_daa_score,
                dns.tkn.settlement_delay_epochs,
            );
            config.params.dns_params = Some(dns);
        } else if !self.tkn_fixture_transfers.is_empty() || !self.tkn_fixture_burns.is_empty() {
            panic!("--tkn-fixture-transfer/--tkn-fixture-burn only mean something together with --tkn-devnet");
        }

        // A malformed checkpoint is fatal on purpose. Continuing without one would leave the node
        // syncing by work alone while its operator believes it is pinned — the one failure mode
        // where a silent fallback is worse than not starting.
        config.trusted_checkpoint = match self.trusted_checkpoint.as_deref() {
            Some(raw) => match raw.parse::<TrustedCheckpoint>() {
                Ok(cp) => Some(cp),
                Err(e) => panic!("--trusted-checkpoint {raw:?} is invalid: {e}"),
            },
            None => None,
        };

        #[cfg(feature = "devnet-prealloc")]
        if let Some(num_prealloc_utxos) = self.num_prealloc_utxos {
            config.initial_utxo_set = Arc::new(self.generate_prealloc_utxos(num_prealloc_utxos));
        }
    }

    #[cfg(feature = "devnet-prealloc")]
    pub fn generate_prealloc_utxos(&self, num_prealloc_utxos: u64) -> kaspa_consensus_core::utxo::utxo_collection::UtxoCollection {
        let addr = Address::try_from(&self.prealloc_address.as_ref().unwrap()[..]).unwrap();
        let spk = pay_to_address_script(&addr);
        (1..=num_prealloc_utxos)
            .map(|i| {
                (
                    TransactionOutpoint { transaction_id: i.into(), index: 0 },
                    UtxoEntry { amount: self.prealloc_amount, script_public_key: spk.clone(), block_daa_score: 0, is_coinbase: false },
                )
            })
            .collect()
    }

    pub fn network(&self) -> NetworkId {
        match (self.testnet, self.devnet, self.simnet) {
            (false, false, false) => NetworkId::new(NetworkType::Mainnet),
            (true, false, false) => NetworkId::with_suffix(NetworkType::Testnet, self.testnet_suffix),
            (false, true, false) => NetworkId::new(NetworkType::Devnet),
            (false, false, true) => NetworkId::new(NetworkType::Simnet),
            _ => panic!("only a single net should be activated"),
        }
    }
}

pub fn cli() -> Command {
    let defaults: Args = Default::default();

    #[allow(clippy::let_and_return)]
    let cmd = Command::new("kaspad")
        .about(format!("{} (misakas) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(arg!(-C --configfile <CONFIG_FILE> "Path of config file.").env("KASPAD_CONFIGFILE"))
        .arg(arg!(-b --appdir <DATA_DIR> "Directory to store data.").env("KASPAD_APPDIR"))
        .arg(arg!(--logdir <LOG_DIR> "Directory to log output.").env("KASPAD_LOGDIR"))
        .arg(arg!(--nologfiles "Disable logging to files.").env("KASPAD_NOLOGFILES"))
        .arg(
            Arg::new("async_threads")
                .short('t')
                .long("async-threads")
                .env("KASPAD_ASYNC_THREADS")
                .value_name("async_threads")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help(format!("Specify number of async threads (default: {}).", defaults.async_threads)),
        )
        .arg(
            Arg::new("log_level")
                .short('d')
                .long("loglevel")
                .env("KASPAD_LOG_LEVEL")
                .value_name("LEVEL")
                .default_value("info")
                .require_equals(true)
                .help("Logging level for all subsystems {off, error, warn, info, debug, trace}\n-- You may also specify <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for individual subsystems.".to_string()),
        )
        .arg(
            Arg::new("rpclisten")
                .long("rpclisten")
                .visible_alias("node-grpc-listen")
                .env("KASPAD_RPCLISTEN")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Interface:port to listen for node gRPC connections — miner / low-level RPC (default port: 26110, testnet: 26210). NOT wRPC Borsh 27210, NOT wRPC JSON 28210, NOT EVM 8545."),
        )
        .arg(
            Arg::new("evm-rpc-listen")
                .long("evm-rpc-listen")
                .visible_alias("evm-rpc-http-listen")
                .env("KASPAD_EVM_RPC_LISTEN")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Interface:port for the Ethereum JSON-RPC HTTP adapter (EVM lane; default port: 8545). Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("evm-history-mode")
                .long("evm-history-mode")
                .env("KASPAD_EVM_HISTORY_MODE")
                .value_name("MODE")
                .value_parser(["head", "recent", "archive"])
                .help("EVM state-history retention: head | recent | archive (default: recent). Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("evm-shadow-state-backend")
                .long("evm-shadow-state-backend")
                .env("KASPAD_EVM_SHADOW_STATE_BACKEND")
                .action(clap::ArgAction::SetTrue)
                .help("C-01: shadow the flat EVM state backend and check it against the committed snapshot every block (HALTS this node on divergence). Node-local, consensus-neutral; off by default. Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("evm-flat-authoritative")
                .long("evm-flat-authoritative")
                .env("KASPAD_EVM_FLAT_AUTHORITATIVE")
                .action(clap::ArgAction::SetTrue)
                .help("C-01 S9: seed the EVM executor from the flat/reconstruct parent state (the cutover seed) instead of the per-block 206 snapshot, after validating it byte-identical to 206 each block (HALTS on divergence; 206 is still written, so it is reversible). Requires --evm-shadow-state-backend. Node-local, consensus-neutral; off by default. Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("evm-retire-206")
                .long("evm-retire-206")
                .env("KASPAD_EVM_RETIRE_206")
                .action(clap::ArgAction::SetTrue)
                .help("C-01 S9b: STOP persisting the per-block 206 EVM state snapshot (the storage win). The flat backend — already checked against the executor's post-state every block — becomes the sole persisted state; RPC and the IBD pruning-point export fall back to flat-materialize / §12-reconstruct. Requires --evm-flat-authoritative; use recent/archive history (not head). Node-local; off by default. Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("evm-prune-legacy-206")
                .long("evm-prune-legacy-206")
                .env("KASPAD_EVM_PRUNE_LEGACY_206")
                .action(clap::ArgAction::SetTrue)
                .help("C-01 S9b-prune: ONE-SHOT, IRREVERSIBLE bulk reclamation at startup of the legacy per-block 206 EVM state snapshots that accumulated before --evm-retire-206 (delete_range + prefix-bounded compaction). Then a no-op. Refused unless --evm-retire-206 is effective (requires --evm-flat-authoritative + --evm-shadow-state-backend). Node-local, consensus-neutral; off by default. Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("evm-materialize-pp-anchor")
                .long("evm-materialize-pp-anchor")
                .env("KASPAD_EVM_MATERIALIZE_PP_ANCHOR")
                .action(clap::ArgAction::SetTrue)
                .help("F2c (t10 recovery): ONE-SHOT startup backfill of the pruning point's EVM state anchor by reverse-replaying the retained §12 diffs from the flat head down to the pruning point. The result is verified against the pruning point's committed state_root BEFORE anything is persisted (a mismatch aborts with nothing written), and a present anchor makes it a no-op. For retired-206 datadirs that predate the automatic pp-anchor step and therefore cannot serve pruned-IBD. Node-local, consensus-neutral; off by default. Effective only in an --features evm build."),
        )
        .arg(
            Arg::new("rpclisten-borsh")
                .long("rpclisten-borsh")
                .visible_alias("node-wrpc-borsh-listen")
                .env("KASPAD_RPCLISTEN_BORSH")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("default") // TODO: Find a way to use defaults.rpclisten_borsh
                .value_parser(clap::value_parser!(WrpcNetAddress))
                .help("Interface:port to listen for node wRPC Borsh connections — validator / wallet / operator (default port: 27110, testnet: 27210). NOT gRPC 26210, NOT wRPC JSON 28210, NOT EVM 8545."),

        )
        .arg(
            Arg::new("rpclisten-json")
                .long("rpclisten-json")
                .visible_alias("node-wrpc-json-listen")
                .env("KASPAD_RPCLISTEN_JSON")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("default") // TODO: Find a way to use defaults.rpclisten_json
                .value_parser(clap::value_parser!(WrpcNetAddress))
                .help("Interface:port to listen for node wRPC JSON connections — explorer / browser (default port: 28110, testnet: 28210). NOT EVM JSON-RPC 8545."),
        )
        .arg(arg!(--unsaferpc "Enable RPC commands which affect the state of the node").env("KASPAD_UNSAFERPC"))
        .arg(
            Arg::new("connect-peers")
                .long("connect")
                .env("KASPAD_CONNECTPEERS")
                .value_name("IP[:PORT]")
                .action(ArgAction::Append)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Connect only to the specified peers at startup."),
        )
        .arg(
            Arg::new("add-peers")
                .long("addpeer")
                .visible_alias("peer")
                .env("KASPAD_ADDPEERS")
                .value_name("IP[:PORT]")
                .action(ArgAction::Append)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Add P2P peers to connect with at startup (this is a P2P address, not an RPC endpoint)."),
        )
        .arg(
            Arg::new("listen")
                .long("listen")
                .visible_alias("p2p-listen")
                .env("KASPAD_LISTEN")
                .value_name("IP[:PORT]")
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Add an interface:port to listen for P2P connections — node-to-node only, NOT an RPC port (default all interfaces port: 26111, testnet: 26211)."),
        )
        .arg(
            Arg::new("outpeers")
                .long("outpeers")
                .env("KASPAD_OUTPEERS")
                .value_name("outpeers")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("Target number of outbound peers (default: 8)."),
        )
        .arg(
            Arg::new("maxinpeers")
                .long("maxinpeers") 
                .env("KASPAD_MAXINPEERS")
                .value_name("maxinpeers")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("Max number of inbound peers (default: 128)."),
        )
        .arg(
            Arg::new("rpcmaxclients")
                .long("rpcmaxclients")
                .env("KASPAD_RPCMAXCLIENTS")
                .value_name("rpcmaxclients")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("Max number of RPC clients for standard connections (default: 128)."),
        )
        .arg(arg!(--"reset-db" "Reset database before starting node. It's needed when switching between subnetworks.").env("KASPAD_RESET_DB"))
        .arg(arg!(--"enable-unsynced-mining" "Allow the node to accept blocks from RPC while not synced (this flag is mainly used for testing)").env("KASPAD_ENABLE_UNSYNCED_MINING"))
        .arg(
            Arg::new("enable-mainnet-mining")
                .long("enable-mainnet-mining")
                .env("KASPAD_ENABLE_MAINNET_MINING")
                .action(ArgAction::SetTrue)
                .hide(true)
                .help("Allow mainnet mining (currently enabled by default while the flag is kept for backwards compatibility)"),
        )
        .arg(
            Arg::new("trusted-checkpoint")
                .long("trusted-checkpoint")
                .value_name("daa:hash:params-id")
                .require_equals(false)
                .help(
                    "kaspa-pq: the history this operator vouches for, as <daa-score>:<block-hash>:<consensus-params-id>. \
                     Chains that do not descend from this block are refused during IBD, whatever work they claim. A node \
                     with no chain of its own cannot tell two internally consistent histories apart, so on a network that \
                     has forked this is what decides which one it may join. Unset means work alone decides.",
                )
                .env("KASPAD_TRUSTED_CHECKPOINT"),
        )
        .arg(
            arg!(--"enforce-chain-participation" "kaspa-pq: enforce the post-IBD chain-participation gate on networks where it is off by default (devnet/simnet). Always enforced on mainnet and testnet.")
                .env("KASPAD_ENFORCE_CHAIN_PARTICIPATION"),
        )
        .arg(
            arg!(--"clear-quarantine" "kaspa-pq (ADR-0025): operator override — clear a persisted Quarantined chain-participation state at startup and resume normal participation. Clears quarantine ONLY (a pending candidate review keeps its deadline). Fires on EVERY boot it is present for; remove it from the service unit once the node is back.")
                .env("KASPAD_CLEAR_QUARANTINE"),
        )
        .arg(arg!(--"enable-validator" "kaspa-pq: run the in-process DNS-overlay validator service (ADR-0010). Default off.").env("KASPAD_ENABLE_VALIDATOR"))
        .arg(
            arg!(--"palw-produce" "PALW ADR-0042: run the in-process PALW-RC block producer. Needs --palw-producer-key, --palw-producer-bond and --palw-producer-pay-address. Only a ConsensusV2 network can use it. Default off.")
                .env("KASPAD_PALW_PRODUCE"),
        )
        .arg(
            Arg::new("palw-producer-key")
                .long("palw-producer-key")
                .env("KASPAD_PALW_PRODUCER_KEY")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("PALW: path to the 32-byte hex ML-DSA-87 seed whose VERIFICATION key the bond registered — a genesis bond, or one this node made with --palw-register-bond. Generate the seed with `misaka key gen`; this never creates one."),
        )
        .arg(
            Arg::new("palw-producer-bond")
                .long("palw-producer-bond")
                .env("KASPAD_PALW_PRODUCER_BOND")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("PALW: <txid>:<index> of the bond output this node signs attempts under. A genesis bond names it on the card; a bond made with --palw-register-bond prints it when the carrier is submitted, and that line is the only place it appears — the outpoint is the carrier's own id."),
        )
        .arg(
            Arg::new("palw-drill-challenge-all")
                .long("palw-drill-challenge-all")
                .env("KASPAD_PALW_DRILL_CHALLENGE_ALL")
                .action(clap::ArgAction::SetTrue)
                .help(
                    "PALW DRILL ONLY: open a court against every licensed claim, including ones this node reproduces \
                     exactly. Exists so an HONEST producer can be shown clearing itself — the half of a round trip that a \
                     conviction alone does not prove. Every such dispute costs this bond the claim's stake and loses. \
                     REFUSED on mainnet.",
                ),
        )
        .arg(
            Arg::new("palw-drill-tamper-leaf")
                .long("palw-drill-tamper-leaf")
                .env("KASPAD_PALW_DRILL_TAMPER_LEAF")
                .require_equals(true)
                .value_parser(clap::value_parser!(u64))
                .help(
                    "PALW DRILL ONLY: produce blocks whose committed execution has one lane of this step leaf corrupted, \
                     with the commitment re-derived so the fraud is self-consistent and only a re-execution can see it. \
                     Exists so a court can be shown convicting on a live chain. REFUSED on mainnet.",
                ),
        )
        .arg(
            Arg::new("palw-challenge")
                .long("palw-challenge")
                .env("KASPAD_PALW_CHALLENGE")
                .action(clap::ArgAction::SetTrue)
                .help(
                    "PALW: re-run every licensed claim and open a court against any this node cannot reproduce. Costs one \
                     inference per claim and stakes this bond the claim's own reserved amount on every dispute it opens, \
                     so it is a watchdog role rather than something every seat does.",
                ),
        )
        .arg(
            Arg::new("palw-producer-class")
                .long("palw-producer-class")
                .value_name("HEX")
                .help(
                    "MISAKA PALW: produce for this 128-hex class id instead of the network's floor class. The node \
                     must be able to resolve it (the floor is derived; any other class needs its --palw-class-artifact).",
                ),
        )
        .arg(
            Arg::new("palw-class-cache-bytes")
                .long("palw-class-cache-bytes")
                .value_name("bytes")
                .require_equals(false)
                .value_parser(clap::value_parser!(u64))
                .help(
                    "MISAKA PALW: hold at most this many bytes of --palw-class-artifact weights (0 = unbounded, the \
                     default). Artifacts load in the order given -- your priority -- and loading stops at the bound; \
                     what did not fit is named in the log. A class this node does not hold is one it does not declare \
                     and is not drawn to judge, so a bound too small for the classes you meant to serve costs you \
                     draws rather than correctness.",
                ),
        )
        .arg(
            Arg::new("palw-register-class")
                .long("palw-register-class")
                .num_args(0..=1)
                .default_missing_value("")
                .require_equals(false)
                .value_name("model-id")
                .help(
                    "MISAKA PALW: submit ONE ClassRegistered for the class of this node's --palw-class-artifact, so a model can \
                     join a chain that is already running instead of waiting for a re-mint. Needs an active bond \
                     (--palw-producer-bond), its key and a funded --palw-fee-outpoint. Give a model id (e.g. \
                     \"Qwen/Qwen2.5-Coder-1.5B-Instruct\") when the artifact's shape matches more than one class this build \
                     knows — sibling models share a converted shape, so the file alone cannot say which one it is.",
                ),
        )
        .arg(
            Arg::new("palw-class-carriage")
                .long("palw-class-carriage")
                .action(ArgAction::Append)
                .value_name("class-id:file")
                .help(
                    "MISAKA PALW (ADR-0067 Decision 6): adopt a class DECLARATION this node did not watch arrive — the \
                     pruned-sync path, where the class table arrives wholesale and no declaration with it. The file is \
                     the borsh PalwClassAdmissionCarriageV2 the registration carried. It needs no trust in whoever \
                     supplied it: the node refuses unless the chain currently holds that class unfrozen, the profile \
                     hashes to the class id, and the canonical job names the same class. Repeatable.",
                ),
        )
        .arg(
            Arg::new("palw-chain-classes")
                .long("palw-chain-classes")
                .action(ArgAction::SetTrue)
                .help(
                    "MISAKA PALW (ADR-0067): ARM the chain-registered-class arm — serve classes whose declaration \
                     the chain carries even when this build's tables never heard of them, executing FROM the \
                     registered profile. Off by default (the fence): arming accepts interpreted execution for \
                     stranger classes this operator's artifacts can pair with. Registration never obligates \
                     possession — a class is served only if its artifact is loaded via --palw-class-artifact.",
                ),
        )
        .arg(
            Arg::new("palw-dump-classes")
                .long("palw-dump-classes")
                .action(ArgAction::SetTrue)
                .help(
                    "MISAKA PALW: once synced, log every class this chain holds with its status, share and this \
                     epoch's budget, then keep running. The pair (share, budget) is what gates a producer, and \
                     `GetPalwProducerFacts` returns only the budget — so a node that holds forever could not say \
                     whether its class was never granted share or merely has no row in this epoch's table. Those \
                     are different faults. Read-only.",
                ),
        )
        .arg(
            Arg::new("palw-register-bond")
                .long("palw-register-bond")
                .action(ArgAction::SetTrue)
                .help(
                    "MISAKA PALW: submit ONE BondRegistered for this node's own key, locking collateral from a \
                     confirmed UTXO at --palw-producer-pay-address, then print the bond outpoint and stop. This is \
                     how a node that is on no genesis registry becomes able to produce at all. Needs \
                     --palw-producer-key and --palw-producer-pay-address; the bond it creates is what you then pass \
                     as --palw-producer-bond.",
                ),
        )
        .arg(
            Arg::new("palw-bond-collateral")
                .long("palw-bond-collateral")
                .value_name("sompi")
                .value_parser(clap::value_parser!(u64))
                .help(
                    "MISAKA PALW: collateral to lock in --palw-register-bond. Defaults to what ONE claim on this \
                     chain's floor class currently needs, which is above the chain's bare minimum: the minimum buys \
                     a bond whose exposure ceiling may not fit a single claim, and such a producer holds forever \
                     having locked real money. More collateral is more claims open at once.",
                ),
        )
        .arg(
            Arg::new("palw-class-artifact")
                .long("palw-class-artifact")
                .env("KASPAD_PALW_CLASS_ARTIFACT")
                .require_equals(true)
                .action(clap::ArgAction::Append)
                .value_parser(clap::value_parser!(String))
                .help(
                    "PALW: path to a converted class artifact (repeatable). Only needed for a class whose weights are \
                     not derivable — the floor is minted from a seed on every node. Digest-checked on load and matched \
                     against the class the chain registered.",
                ),
        )
        .arg(
            Arg::new("palw-producer-pay-address")
                .long("palw-producer-pay-address")
                .env("KASPAD_PALW_PRODUCER_PAY_ADDRESS")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("PALW: where produced blocks pay their reward. Must be an ML-DSA-87 P2PKH address — PQ-only consensus rejects anything else, and the block would be dead on arrival."),
        )
        .arg(
            Arg::new("palw-heartbeat-miner-address")
                .long("palw-heartbeat-miner-address")
                .env("KASPAD_PALW_HEARTBEAT_MINER_ADDRESS")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("ADR-0060: run the bondless heartbeat miner (algo-3, one CPU thread, fee-only — the lane that keeps the chain's clock alive when every bonded lane is silent), paying its fees to this ML-DSA-87 P2PKH address. Only a ConsensusV2 network has the lane. Default off."),
        )
        .arg(
            arg!(--"palw-panel" "PALW ADR-0042 Decision 7: run the in-process panel service — verify gossiped claim material against the claims this node's bond is seated on, sign and broadcast receipts, and (when --palw-fee-outpoint is set) submit the assembled quorum to the chain. Uses --palw-producer-key and --palw-producer-bond for the seat identity; --palw-produce is NOT required. Only a ConsensusV2 network can use it. Default off.")
                .env("KASPAD_PALW_PANEL"),
        )
        .arg(
            Arg::new("palw-fee-outpoint")
                .long("palw-fee-outpoint")
                .env("KASPAD_PALW_FEE_OUTPOINT")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("PALW: <txid>:<index> of a UTXO paying to the bond key's own P2PKH address, spent to carry lifecycle submissions (ReceiptLicensed / ProducerDefaulted). Change returns to the same address and the rolling outpoint is persisted, so one funding covers many submissions. Without it the panel signs receipts but submits nothing."),
        )
        .arg(
            Arg::new("evm-fee-recipient")
                .long("evm-fee-recipient")
                .env("KASPAD_EVM_FEE_RECIPIENT")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("kaspa-pq EVM Lane: the miner's EVM coinbase address (20-byte hex, optional 0x) — receives the priority fees of this node's own EVM payload txs."),
        )
        .arg(
            Arg::new("validator-key")
                .long("validator-key")
                .env("KASPAD_VALIDATOR_KEY")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("kaspa-pq: path to the validator ML-DSA-87 signing seed file (64 hex chars = 32 bytes)."),
        )
        .arg(
            Arg::new("stake-bond")
                .long("stake-bond")
                .env("KASPAD_STAKE_BOND")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("kaspa-pq: stake-bond outpoint backing this validator's attestations, as 'txid:index'."),
        )
        .arg(
            Arg::new("validator-mode")
                .long("validator-mode")
                .env("KASPAD_VALIDATOR_MODE")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("kaspa-pq: validator operating mode {active, standby, observer} (default: observer)."),
        )
        .arg(
            arg!(--"enable-compute" "MISAKA VLT: run the compute role (execute + audit LLM jobs) alongside the validator service. \
                 Requires --enable-validator and --compute-worker; inert on networks whose model cost table is empty. Default off.")
                .env("KASPAD_ENABLE_COMPUTE"),
        )
        .arg(
            Arg::new("compute-worker")
                .long("compute-worker")
                .env("KASPAD_COMPUTE_WORKER")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help(
                    "MISAKA VLT: path to the pinned palw-worker binary. Without it the compute role stays disabled — an \
                     unregistered runtime mints nothing and would refute honest peers if it were drawn as a verifier.",
                ),
        )
        .arg(
            Arg::new("compute-endpoint")
                .long("compute-endpoint")
                .env("KASPAD_COMPUTE_ENDPOINT")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help(
                    "MISAKA PALW v2: path to a palw-agent Unix socket to health-monitor (Land stage: observation and \
                     capability state only; grants no reward, no work, no fork-choice weight, and does not replace \
                     --compute-worker). The node runs validator-only regardless of the agent's state. Unix hosts \
                     only — the agent protocol is AF_UNIX; on Windows the flag is accepted, logs one warning and \
                     leaves compute capability withdrawn.",
                ),
        )
        .arg(
            Arg::new("compute-work-dir")
                .long("compute-work-dir")
                .env("KASPAD_COMPUTE_WORK_DIR")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("MISAKA VLT: scratch directory the compute worker runs in (default: the system temp directory)."),
        )
        .arg(
            Arg::new("compute-prompt")
                .long("compute-prompt")
                .env("KASPAD_COMPUTE_PROMPT")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help(
                    "MISAKA VLT: file holding this node's executor job input. Omit to run verifier-only, auditing peers' jobs \
                     without originating any.",
                ),
        )
        .arg(
            Arg::new("compute-max-tokens")
                .long("compute-max-tokens")
                .env("KASPAD_COMPUTE_MAX_TOKENS")
                .require_equals(true)
                .value_parser(clap::value_parser!(u32))
                .help("MISAKA VLT: token ceiling for this node's own jobs, clamped down to the registered profile's limit."),
        )
        .arg(
            Arg::new("compute-timeout-secs")
                .long("compute-timeout-secs")
                .env("KASPAD_COMPUTE_TIMEOUT_SECS")
                .require_equals(true)
                .value_parser(clap::value_parser!(u64))
                .help("MISAKA VLT: wall-clock ceiling for one compute job, in seconds (default: 900)."),
        )
        .arg(
            arg!(--"compute-auto-challenge" "MISAKA VLT: file a ForgedReceipt fraud proof when a replay refutes a peer. Off by \
                 default: the refuting verdict already blocks the credit, while a challenge stakes this node's own bond on a \
                 divergence that a mis-declared determinism class would also produce.")
                .env("KASPAD_COMPUTE_AUTO_CHALLENGE"),
        )
        .arg(
            Arg::new("compute-fixture-job-limit")
                .long("compute-fixture-job-limit")
                .value_name("N")
                .value_parser(clap::value_parser!(u32))
                .require_equals(false)
                .help(
                    "MISAKA devnet fixture: originate at most N JOBS — not N VLT — ever, then stop. The fixture runs \
                     one fixed job shape worth exactly 50 VLT, so a plan of 400/250/150/100/100 VLT is N = 8/5/3/2/2. \
                     The count is persisted next to the compute work dir, so a restart does not reset it. This is what \
                     makes an ASYMMETRIC weight experiment possible: five validators running the same fixed job differ \
                     only in how many they complete. Without it an executor keeps originating forever and every \
                     validator converges on the same weight.",
                )
                .env("KASPAD_COMPUTE_FIXTURE_JOB_LIMIT"),
        )
        .arg(
            Arg::new("vlt-devnet")
                .long("vlt-devnet")
                .value_name("shadow-daa-score")
                .value_parser(clap::value_parser!(u64))
                .require_equals(false)
                .help(
                    "MISAKA VLT: activate the compute overlay on a PRIVATE devnet, with the shadow fence at this DAA score \
                     and the weight fence one full credit window above it. Also registers the shipped PALW model, without \
                     which every job would mint zero. DEVNET/SIMNET ONLY — on a public network the fences are a release \
                     decision, not a node-operator one, and the node refuses to start.",
                )
                .env("KASPAD_VLT_DEVNET"),
        )
        .arg(
            Arg::new("vlt-devnet-credit-window-epochs")
                .long("vlt-devnet-credit-window-epochs")
                .value_name("K")
                .value_parser(clap::value_parser!(u32))
                .require_equals(false)
                .help(
                    "MISAKA VLT: K for --vlt-devnet (default 8, against production's 96). K sets both the credit walk's \
                     depth and the soak between the two fences, so a smaller K is a devnet that reaches weighted finality \
                     in minutes instead of tens of minutes.",
                )
                .env("KASPAD_VLT_DEVNET_CREDIT_WINDOW_EPOCHS"),
        )
        .arg(
            Arg::new("tkn-devnet")
                .long("tkn-devnet")
                .value_name("active-daa-score")
                .value_parser(clap::value_parser!(u64))
                .require_equals(false)
                .help(
                    "MISAKA Compute Token Program: open the TOK ledger fold and emission on a PRIVATE devnet at this DAA \
                     score, with the shadow fence --tkn-devnet-shadow-span below it. Requires --vlt-devnet (emission \
                     settles over VLT credits; a token program on an inert compute overlay is undefined). DEVNET/SIMNET \
                     ONLY — on a public network the fences are a release decision, and the node refuses to start.",
                )
                .env("KASPAD_TKN_DEVNET"),
        )
        .arg(
            Arg::new("tkn-devnet-shadow-span")
                .long("tkn-devnet-shadow-span")
                .value_name("daa-span")
                .value_parser(clap::value_parser!(u64))
                .require_equals(false)
                .help(
                    "MISAKA TOK: how far below the --tkn-devnet active fence the shadow fence sits (default 300 DAA). \
                     The [shadow, active) window is where the harness proves shadow-era ops stay void forever.",
                )
                .env("KASPAD_TKN_DEVNET_SHADOW_SPAN"),
        )
        .arg(
            Arg::new("tkn-devnet-epoch-budget-tok")
                .long("tkn-devnet-epoch-budget-tok")
                .value_name("tok-per-epoch")
                .value_parser(clap::value_parser!(u64))
                .require_equals(false)
                .help("MISAKA TOK: flat per-epoch emission budget in whole TOK for --tkn-devnet (default 1000; no halving within a run)."),
        )
        .arg(
            Arg::new("tkn-fixture-transfer")
                .long("tkn-fixture-transfer")
                .value_name("to-hex128:amount-atomic:nonce:at-daa")
                .action(clap::ArgAction::Append)
                .require_equals(false)
                .help(
                    "MISAKA TOK devnet fixture: once the chain reaches at-daa, sign and submit ONE TOK transfer from this \
                     node's validator identity. Repeatable; nonce is taken literally so a harness can submit deliberately \
                     void ops (bad nonce, overdraft) and assert they stay void.",
                ),
        )
        .arg(
            Arg::new("tkn-fixture-burn")
                .long("tkn-fixture-burn")
                .value_name("amount-atomic:nonce:at-daa")
                .action(clap::ArgAction::Append)
                .require_equals(false)
                .help("MISAKA TOK devnet fixture: once the chain reaches at-daa, sign and submit ONE TOK burn. Repeatable."),
        )
        .arg(
            arg!(--"vlt-shadow-only" "MISAKA VLT Shadow Mode: with --vlt-devnet, leave the WEIGHT fence dormant. The overlay \
                 runs and is policed for real — certificates credited, committees drawn, verdicts paid, challenges slashing — \
                 while DNS finality stays on bonded stake indefinitely. This is the mode to run before committing to a \
                 weight fence: it produces the C_i(E) you need to see before deciding it is safe to vote on.")
                .env("KASPAD_VLT_SHADOW_ONLY"),
        )
        .arg(
            arg!(--"vlt-devnet-flat-decay" "MISAKA VLT: with --vlt-devnet, pin the credit decay flat (d_tau = 1). A job-quota \
                 weight plan (e.g. 8/5/3/2/2 jobs at 50 VLT each) then lands as exactly its intended weights, whichever epoch \
                 each validator finished its quota in. Devnet calibration only — production keeps real decay.")
                .env("KASPAD_VLT_DEVNET_FLAT_DECAY"),
        )
        .arg(arg!(--utxoindex "Enable the UTXO index").env("KASPAD_UTXOINDEX"))
        .arg(
            Arg::new("max-tracked-addresses")
                .long("max-tracked-addresses")
                .env("KASPAD_MAX_TRACKED_ADDRESSES")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help(format!("Max (preallocated) number of addresses being tracked for UTXO changed events (default: {}, maximum: {}). 
Setting to 0 prevents the preallocation and sets the maximum to {}, leading to 0 memory footprint as long as unused but to sub-optimal footprint if used.", 
0, Tracker::MAX_ADDRESS_UPPER_BOUND, Tracker::DEFAULT_MAX_ADDRESSES)),
        )
        .arg(arg!(--testnet "Use the test network").env("KASPAD_TESTNET"))
        .arg(
            Arg::new("netsuffix")
                .long("netsuffix")
                .env("KASPAD_NETSUFFIX")
                .value_name("netsuffix")
                .require_equals(true)
                .value_parser(clap::value_parser!(u32))
                .help("Testnet network suffix number"),
        )
        .arg(arg!(--devnet "Use the development test network").env("KASPAD_DEVNET"))
        .arg(arg!(--simnet "Use the simulation test network").env("KASPAD_SIMNET"))
        .arg(arg!(--archival "Run as an archival node: avoids deleting old block data when moving the pruning point (Warning: heavy disk usage)").env("KASPAD_ARCHIVAL"))
        .arg(arg!(--sanity "Enable various sanity checks which might be compute-intensive (mostly performed during pruning)").env("KASPAD_SANITY"))
        .arg(arg!(--yes "Answer yes to all interactive console questions").env("KASPAD_NONINTERACTIVE"))
        .arg(
            Arg::new("user_agent_comments")
                .long("uacomment")
                .env("KASPAD_USER_AGENT_COMMENTS")
                .action(ArgAction::Append)
                .require_equals(true)
                .help("Comment to add to the user agent -- See BIP 14 for more information."),
        )
        .arg(
            Arg::new("externalip")
                .long("externalip")
                .env("KASPAD_EXTERNALIP")
                .value_name("externalip")
                .require_equals(true)
                .default_missing_value(None)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Add a socket address(ip:port) to the list of local addresses we claim to listen on to peers"),
        )
        .arg(arg!(--"perf-metrics" "Enable performance metrics: cpu, memory, disk io usage").env("KASPAD_PERF_METRICS"))
        .arg(
            Arg::new("perf-metrics-interval-sec")
                .long("perf-metrics-interval-sec")
                .env("KASPAD_PERF_METRICS_INTERVAL_SEC")
                .require_equals(true)
                .value_parser(clap::value_parser!(u64))
                .help("Interval in seconds for performance metrics collection."),
        )
        .arg(arg!(--"disable-upnp" "Disable upnp").env("KASPAD_DISABLE_UPNP"))
        .arg(arg!(--"nodnsseed" "Disable DNS seeding for peers").env("KASPAD_NODNSSEED"))
        .arg(arg!(--"nogrpc" "Disable gRPC server").env("KASPAD_NOGRPC"))
        .arg(
            Arg::new("ram-scale")
                .long("ram-scale")
                .env("KASPAD_RAM_SCALE")
                .require_equals(true)
                .value_parser(clap::value_parser!(f64))
                .help("Apply a scale factor to memory allocation bounds. Nodes with limited RAM (~4-8GB) should set this to ~0.3-0.5 respectively. Nodes with
a large RAM (~64GB) can set this value to ~3.0-4.0 and gain superior performance especially for syncing peers faster"),
        )
        .arg(
            Arg::new("retention-period-days")
                .long("retention-period-days")
                .require_equals(true)
                .value_parser(clap::value_parser!(f64))
                .help("The number of total days of data to keep.")
        )
        .arg(
            Arg::new("override-params-file")
                .long("override-params-file")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("Path to a JSON file containing override parameters.")
        )
        .arg(
            Arg::new("node-profile")
                .long("node-profile")
                .env("KASPAD_NODE_PROFILE")
                .require_equals(true)
                .value_parser(NodeProfile::VARIANTS)
                .help("MISAKA node role profile: full | bootstrap-pruned | recovery-sync | validator | archive | public-rpc. \
                       The sync-only profiles apply 8GB resource defaults and reject --archival/--utxoindex/--enable-validator/\
                       --evm-rpc-listen/--unsaferpc; recovery-sync additionally requires --connect. Consensus rules are unchanged.")
        )
        .arg(
            Arg::new("vps-8gb")
                .long("vps-8gb")
                .env("KASPAD_VPS_8GB")
                .action(ArgAction::SetTrue)
                .help("Apply 8GB-VPS resource defaults for unspecified knobs: ram-scale=0.3, async-threads=2, outpeers=4, \
                       maxinpeers=32, rpcmaxclients=8, nogrpc, min-disk-free-percent=15. Warns when system memory is below 7.5GB.")
        )
        .arg(
            Arg::new("min-disk-free-percent")
                .long("min-disk-free-percent")
                .env("KASPAD_MIN_DISK_FREE_PERCENT")
                .require_equals(true)
                .value_parser(clap::value_parser!(u8))
                .help("Refuse startup when free disk on the data mount is below this percentage. 0 disables; sync-only profiles and --vps-8gb default to 15.")
        )
        .arg(
            Arg::new("profile")
                .long("profile")
                .env("KASPAD_PROFILE")
                .require_equals(true)
                .value_parser(["minimal", "local-validator", "local-full", "public-evm-rpc", "public-node-rpc"])
                .help("RPC profile — a named bundle of listeners (design §9): \
                       minimal (P2P + gRPC only) | local-validator (+ wRPC Borsh, loopback) | \
                       local-full (+ wRPC JSON + EVM HTTP, loopback) | public-evm-rpc (EVM HTTP on 0.0.0.0; \
                       still gated by MISAKA_ALLOW_PUBLIC_EVM_RPC) | public-node-rpc (wRPC JSON on 0.0.0.0). \
                       Explicit --rpclisten* / --evm-rpc-listen always override the profile.")
        )
        .arg(
            arg!(--"allow-public-rpc" "Acknowledge binding the node RPC (gRPC / wRPC Borsh / wRPC JSON) to a non-loopback address. Without it a public RPC bind still works but logs a security warning at startup (not a fail-closed refusal). The EVM RPC keeps its own MISAKA_ALLOW_PUBLIC_EVM_RPC gate.")
                .env("KASPAD_ALLOW_PUBLIC_RPC"),
        )
        .arg(
            Arg::new("rocksdb-preset")
                .long("rocksdb-preset")
                .env("KASPAD_ROCKSDB_PRESET")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("RocksDB configuration preset: 'default' (SSD/NVMe) or 'hdd' (optimized for hard disk drives with BlobDB, compression, rate limiting). \
                       HDD preset recommended for archival nodes on HDD storage (see docs/archival.md).")
        )
        .arg(
            Arg::new("rocksdb-wal-dir")
                .long("rocksdb-wal-dir")
                .env("KASPAD_ROCKSDB_WAL_DIR")
                .require_equals(true)
                .value_parser(clap::value_parser!(String))
                .help("Custom WAL (Write-Ahead Log) directory for RocksDB. Useful for hybrid setups: database on HDD, WAL on fast NVMe SSD. \
                       Example: --rocksdb-wal-dir=/mnt/nvme/kaspa-wal")
        )
        .arg(
            Arg::new("rocksdb-cache-size")
                .long("rocksdb-cache-size")
                .env("KASPAD_ROCKSDB_CACHE_SIZE")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("RocksDB block cache size in MB. Default: 256MB for HDD preset (scales with --ram-scale). \
                       Increase for public RPC nodes with heavy query loads. Example: --rocksdb-cache-size=2048 for 2GB cache.")
        )
        ;

    #[cfg(feature = "devnet-prealloc")]
    let cmd = cmd
        .arg(Arg::new("num-prealloc-utxos").long("num-prealloc-utxos").require_equals(true).value_parser(clap::value_parser!(u64)))
        .arg(Arg::new("prealloc-address").long("prealloc-address").require_equals(true).value_parser(clap::value_parser!(String)))
        .arg(Arg::new("prealloc-amount").long("prealloc-amount").require_equals(true).value_parser(clap::value_parser!(u64)));

    cmd
}

pub fn parse_args() -> Args {
    match Args::parse(std::env::args_os()) {
        Ok(args) => args,
        // `--help` and `--version` arrive HERE, as `Err`. clap models them as errors only to
        // carry the rendered text; `ErrorKind::DisplayHelp`/`DisplayVersion` are answers, not
        // failures. The old arm could not tell the difference — it printed everything to
        // stdout and exited 1 — so `kaspad --version` reported failure to anything that reads
        // an exit code. The VPS setup wizard's own probe was one of them, and it has been
        // logging "kaspad check did not finish cleanly" on a perfectly healthy binary.
        //
        // `Error::exit` is where that distinction already lives, and it is what every other
        // binary in this workspace gets for free from clap's derive `Parser::parse`: help and
        // version to stdout with status 0, a usage error to stderr with clap's usage status
        // (2, not the 1 this printed). Re-deciding it here is what produced the bug.
        Err(err) => err.exit(),
    }
}

impl Args {
    pub fn parse<I, T>(itr: I) -> Result<Args, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let m: clap::ArgMatches = cli().try_get_matches_from(itr)?;
        let mut defaults: Args = Default::default();

        if let Some(config_file) = m.get_one::<String>("configfile") {
            let config_str = fs::read_to_string(config_file)?;
            defaults = from_str(&config_str).map_err(|toml_error| {
                clap::Error::raw(
                    clap::error::ErrorKind::ValueValidation,
                    format!("failed parsing config file, reason: {}", toml_error.message()),
                )
            })?;
        }

        let cfg_baseline = defaults.clone();
        let mut args = Args {
            appdir: m.get_one::<String>("appdir").cloned().or(defaults.appdir),
            logdir: m.get_one::<String>("logdir").cloned().or(defaults.logdir),
            no_log_files: arg_match_unwrap_or::<bool>(&m, "nologfiles", defaults.no_log_files),
            rpclisten: m.get_one::<ContextualNetAddress>("rpclisten").cloned().or(defaults.rpclisten),
            evm_rpc_listen: m.get_one::<ContextualNetAddress>("evm-rpc-listen").cloned().or(defaults.evm_rpc_listen),
            evm_history_mode: m
                .get_one::<String>("evm-history-mode")
                .and_then(|s| EvmHistoryMode::from_str_opt(s))
                .unwrap_or(defaults.evm_history_mode),
            evm_shadow_state_backend: arg_match_unwrap_or::<bool>(&m, "evm-shadow-state-backend", defaults.evm_shadow_state_backend),
            evm_flat_authoritative: arg_match_unwrap_or::<bool>(&m, "evm-flat-authoritative", defaults.evm_flat_authoritative),
            evm_retire_206: arg_match_unwrap_or::<bool>(&m, "evm-retire-206", defaults.evm_retire_206),
            evm_prune_legacy_206: arg_match_unwrap_or::<bool>(&m, "evm-prune-legacy-206", defaults.evm_prune_legacy_206),
            evm_materialize_pp_anchor: arg_match_unwrap_or::<bool>(
                &m,
                "evm-materialize-pp-anchor",
                defaults.evm_materialize_pp_anchor,
            ),
            rpclisten_borsh: m.get_one::<WrpcNetAddress>("rpclisten-borsh").cloned().or(defaults.rpclisten_borsh),
            rpclisten_json: m.get_one::<WrpcNetAddress>("rpclisten-json").cloned().or(defaults.rpclisten_json),
            unsafe_rpc: arg_match_unwrap_or::<bool>(&m, "unsaferpc", defaults.unsafe_rpc),
            wrpc_verbose: false,
            log_level: arg_match_unwrap_or::<String>(&m, "log_level", defaults.log_level),
            async_threads: arg_match_unwrap_or::<usize>(&m, "async_threads", defaults.async_threads),
            connect_peers: arg_match_many_unwrap_or::<ContextualNetAddress>(&m, "connect-peers", defaults.connect_peers),
            add_peers: arg_match_many_unwrap_or::<ContextualNetAddress>(&m, "add-peers", defaults.add_peers),
            listen: m.get_one::<ContextualNetAddress>("listen").cloned().or(defaults.listen),
            outbound_target: arg_match_unwrap_or::<usize>(&m, "outpeers", defaults.outbound_target),
            inbound_limit: arg_match_unwrap_or::<usize>(&m, "maxinpeers", defaults.inbound_limit),
            rpc_max_clients: arg_match_unwrap_or::<usize>(&m, "rpcmaxclients", defaults.rpc_max_clients),
            max_tracked_addresses: arg_match_unwrap_or::<usize>(&m, "max-tracked-addresses", defaults.max_tracked_addresses),
            reset_db: arg_match_unwrap_or::<bool>(&m, "reset-db", defaults.reset_db),
            enable_unsynced_mining: arg_match_unwrap_or::<bool>(&m, "enable-unsynced-mining", defaults.enable_unsynced_mining),
            enable_mainnet_mining: arg_match_unwrap_or::<bool>(&m, "enable-mainnet-mining", defaults.enable_mainnet_mining),
            trusted_checkpoint: m.get_one::<String>("trusted-checkpoint").cloned(),
            clear_quarantine: arg_match_unwrap_or::<bool>(&m, "clear-quarantine", defaults.clear_quarantine),
            enforce_chain_participation: arg_match_unwrap_or::<bool>(
                &m,
                "enforce-chain-participation",
                defaults.enforce_chain_participation,
            ),
            enable_validator: arg_match_unwrap_or::<bool>(&m, "enable-validator", defaults.enable_validator),
            validator_key: m.get_one::<String>("validator-key").cloned().or(defaults.validator_key),
            palw_produce: arg_match_unwrap_or::<bool>(&m, "palw-produce", defaults.palw_produce),
            palw_panel: arg_match_unwrap_or::<bool>(&m, "palw-panel", defaults.palw_panel),
            palw_fee_outpoint: m.get_one::<String>("palw-fee-outpoint").cloned().or(defaults.palw_fee_outpoint),
            palw_producer_key: m.get_one::<String>("palw-producer-key").cloned().or(defaults.palw_producer_key),
            palw_producer_bond: m.get_one::<String>("palw-producer-bond").cloned().or(defaults.palw_producer_bond),
            palw_class_artifact: m
                .get_many::<String>("palw-class-artifact")
                .map(|v| v.cloned().collect())
                .unwrap_or(defaults.palw_class_artifact),
            palw_class_cache_bytes: m.get_one::<u64>("palw-class-cache-bytes").copied().unwrap_or(defaults.palw_class_cache_bytes),
            palw_register_class: m.get_one::<String>("palw-register-class").cloned().or(defaults.palw_register_class.clone()),
            palw_register_bond: arg_match_unwrap_or::<bool>(&m, "palw-register-bond", defaults.palw_register_bond),
            palw_dump_classes: arg_match_unwrap_or::<bool>(&m, "palw-dump-classes", defaults.palw_dump_classes),
            palw_chain_classes: arg_match_unwrap_or::<bool>(&m, "palw-chain-classes", defaults.palw_chain_classes),
            palw_class_carriage: m
                .get_many::<String>("palw-class-carriage")
                .map(|v| v.cloned().collect())
                .unwrap_or(defaults.palw_class_carriage.clone()),
            palw_bond_collateral: m.get_one::<u64>("palw-bond-collateral").copied(),
            palw_producer_class: m.get_one::<String>("palw-producer-class").cloned().or(defaults.palw_producer_class),
            palw_challenge: m.get_one::<bool>("palw-challenge").copied().unwrap_or(defaults.palw_challenge),
            palw_drill_tamper_leaf: m.get_one::<u64>("palw-drill-tamper-leaf").copied().or(defaults.palw_drill_tamper_leaf),
            palw_drill_challenge_all: m
                .get_one::<bool>("palw-drill-challenge-all")
                .copied()
                .unwrap_or(defaults.palw_drill_challenge_all),
            palw_producer_pay_address: m
                .get_one::<String>("palw-producer-pay-address")
                .cloned()
                .or(defaults.palw_producer_pay_address),
            palw_heartbeat_miner_address: m
                .get_one::<String>("palw-heartbeat-miner-address")
                .cloned()
                .or(defaults.palw_heartbeat_miner_address),
            evm_fee_recipient: m.get_one::<String>("evm-fee-recipient").cloned().or(defaults.evm_fee_recipient),
            stake_bond: m.get_one::<String>("stake-bond").cloned().or(defaults.stake_bond),
            validator_mode: m.get_one::<String>("validator-mode").cloned().or(defaults.validator_mode),
            enable_compute: arg_match_unwrap_or::<bool>(&m, "enable-compute", defaults.enable_compute),
            compute_worker: m.get_one::<String>("compute-worker").cloned().or(defaults.compute_worker),
            compute_endpoint: m.get_one::<String>("compute-endpoint").cloned().or(defaults.compute_endpoint),
            compute_work_dir: m.get_one::<String>("compute-work-dir").cloned().or(defaults.compute_work_dir),
            compute_prompt: m.get_one::<String>("compute-prompt").cloned().or(defaults.compute_prompt),
            compute_max_tokens: m.get_one::<u32>("compute-max-tokens").copied().or(defaults.compute_max_tokens),
            compute_timeout_secs: m.get_one::<u64>("compute-timeout-secs").copied().or(defaults.compute_timeout_secs),
            compute_auto_challenge: arg_match_unwrap_or::<bool>(&m, "compute-auto-challenge", defaults.compute_auto_challenge),
            compute_fixture_job_limit: m.get_one::<u32>("compute-fixture-job-limit").copied(),
            vlt_devnet_shadow_daa: m.get_one::<u64>("vlt-devnet").copied(),
            vlt_devnet_credit_window_epochs: arg_match_unwrap_or::<u32>(
                &m,
                "vlt-devnet-credit-window-epochs",
                defaults.vlt_devnet_credit_window_epochs,
            ),
            vlt_shadow_only: arg_match_unwrap_or::<bool>(&m, "vlt-shadow-only", defaults.vlt_shadow_only),
            vlt_devnet_flat_decay: arg_match_unwrap_or::<bool>(&m, "vlt-devnet-flat-decay", defaults.vlt_devnet_flat_decay),
            tkn_devnet_active_daa: m.get_one::<u64>("tkn-devnet").copied(),
            tkn_devnet_shadow_span: arg_match_unwrap_or::<u64>(&m, "tkn-devnet-shadow-span", defaults.tkn_devnet_shadow_span),
            tkn_devnet_epoch_budget_tok: arg_match_unwrap_or::<u64>(
                &m,
                "tkn-devnet-epoch-budget-tok",
                defaults.tkn_devnet_epoch_budget_tok,
            ),
            tkn_fixture_transfers: m.get_many::<String>("tkn-fixture-transfer").map(|v| v.cloned().collect()).unwrap_or_default(),
            tkn_fixture_burns: m.get_many::<String>("tkn-fixture-burn").map(|v| v.cloned().collect()).unwrap_or_default(),
            utxoindex: arg_match_unwrap_or::<bool>(&m, "utxoindex", defaults.utxoindex),
            testnet: arg_match_unwrap_or::<bool>(&m, "testnet", defaults.testnet),
            testnet_suffix: arg_match_unwrap_or::<u32>(&m, "netsuffix", defaults.testnet_suffix),
            devnet: arg_match_unwrap_or::<bool>(&m, "devnet", defaults.devnet),
            simnet: arg_match_unwrap_or::<bool>(&m, "simnet", defaults.simnet),
            archival: arg_match_unwrap_or::<bool>(&m, "archival", defaults.archival),
            sanity: arg_match_unwrap_or::<bool>(&m, "sanity", defaults.sanity),
            yes: arg_match_unwrap_or::<bool>(&m, "yes", defaults.yes),
            user_agent_comments: arg_match_many_unwrap_or::<String>(&m, "user_agent_comments", defaults.user_agent_comments),
            externalip: m.get_one::<ContextualNetAddress>("externalip").cloned(),
            perf_metrics: arg_match_unwrap_or::<bool>(&m, "perf-metrics", defaults.perf_metrics),
            perf_metrics_interval_sec: arg_match_unwrap_or::<u64>(&m, "perf-metrics-interval-sec", defaults.perf_metrics_interval_sec),
            // Note: currently used programmatically by benchmarks and not exposed to CLI users
            block_template_cache_lifetime: defaults.block_template_cache_lifetime,
            disable_upnp: arg_match_unwrap_or::<bool>(&m, "disable-upnp", defaults.disable_upnp),
            disable_dns_seeding: arg_match_unwrap_or::<bool>(&m, "nodnsseed", defaults.disable_dns_seeding),
            disable_grpc: arg_match_unwrap_or::<bool>(&m, "nogrpc", defaults.disable_grpc),
            ram_scale: arg_match_unwrap_or::<f64>(&m, "ram-scale", defaults.ram_scale),
            retention_period_days: m.get_one::<f64>("retention-period-days").cloned().or(defaults.retention_period_days),

            #[cfg(feature = "devnet-prealloc")]
            num_prealloc_utxos: m.get_one::<u64>("num-prealloc-utxos").cloned(),
            #[cfg(feature = "devnet-prealloc")]
            prealloc_address: m.get_one::<String>("prealloc-address").cloned(),
            #[cfg(feature = "devnet-prealloc")]
            prealloc_amount: arg_match_unwrap_or::<u64>(&m, "prealloc-amount", defaults.prealloc_amount),
            override_params_file: m.get_one::<String>("override-params-file").cloned(),
            rocksdb_preset: m.get_one::<String>("rocksdb-preset").cloned().or(defaults.rocksdb_preset),
            rocksdb_wal_dir: m.get_one::<String>("rocksdb-wal-dir").cloned().or(defaults.rocksdb_wal_dir),
            rocksdb_cache_size: m.get_one::<usize>("rocksdb-cache-size").cloned().or(defaults.rocksdb_cache_size),
            node_profile: m.get_one::<String>("node-profile").and_then(|s| NodeProfile::from_cli(s)).unwrap_or(defaults.node_profile),
            vps_8gb: arg_match_unwrap_or::<bool>(&m, "vps-8gb", defaults.vps_8gb),
            min_disk_free_percent: m
                .get_one::<u8>("min-disk-free-percent")
                .cloned()
                .filter(|_| m.value_source("min-disk-free-percent") != Some(DefaultValue))
                .unwrap_or(defaults.min_disk_free_percent),
            profile: m.get_one::<String>("profile").cloned().or(defaults.profile),
            allow_public_rpc: arg_match_unwrap_or::<bool>(&m, "allow-public-rpc", defaults.allow_public_rpc),
        };

        apply_profile_defaults(&mut args, &m, &cfg_baseline);

        if arg_match_unwrap_or::<bool>(&m, "enable-mainnet-mining", false) {
            println!("\nNOTE: The flag --enable-mainnet-mining is deprecated and defaults to true also w/o explicit setting\n")
        }

        args.apply_profile();
        Ok(args)
    }

    /// Apply the `--profile` bundle (design §9): fill in the default RPC listeners for
    /// the chosen profile, but ONLY where the operator did not set them explicitly — an
    /// explicit `--rpclisten-borsh` / `--rpclisten-json` / `--evm-rpc-listen` always
    /// wins. gRPC is always enabled (it defaults to loopback in the daemon), so the
    /// profiles only need to toggle the Borsh / JSON / EVM listeners. A public profile
    /// binds 0.0.0.0; the EVM public bind is still fail-closed behind
    /// `MISAKA_ALLOW_PUBLIC_EVM_RPC` in the daemon.
    fn apply_profile(&mut self) {
        let Some(profile) = self.profile.clone() else { return };
        match profile.as_str() {
            // P2P + gRPC only (both already on by default); nothing extra to enable.
            "minimal" => {}
            "local-validator" => {
                if self.rpclisten_borsh.is_none() {
                    self.rpclisten_borsh = Some(WrpcNetAddress::Default);
                }
            }
            "local-full" => {
                if self.rpclisten_borsh.is_none() {
                    self.rpclisten_borsh = Some(WrpcNetAddress::Default);
                }
                if self.rpclisten_json.is_none() {
                    self.rpclisten_json = Some(WrpcNetAddress::Default);
                }
                if self.evm_rpc_listen.is_none() {
                    self.evm_rpc_listen = Some(ContextualNetAddress::loopback());
                }
            }
            "public-evm-rpc" => {
                if self.evm_rpc_listen.is_none() {
                    self.evm_rpc_listen = Some(ContextualNetAddress::unspecified());
                }
            }
            "public-node-rpc" => {
                if self.rpclisten_json.is_none() {
                    self.rpclisten_json = Some(WrpcNetAddress::Public);
                }
            }
            _ => {}
        }
    }
}

fn apply_profile_defaults(args: &mut Args, m: &clap::ArgMatches, cfg: &Args) {
    if !(args.vps_8gb || args.node_profile.is_sync_only()) {
        return;
    }

    let stock = Args::default();
    let cli_set = |id: &str| m.value_source(id).map(|src| src != DefaultValue).unwrap_or(false);

    if !cli_set("ram-scale") && cfg.ram_scale == stock.ram_scale {
        args.ram_scale = VPS_8GB_RAM_SCALE;
    }
    if !cli_set("async_threads") && cfg.async_threads == stock.async_threads {
        args.async_threads = VPS_8GB_ASYNC_THREADS.min(num_cpus::get().max(1));
    }
    if !cli_set("outpeers") && cfg.outbound_target == stock.outbound_target {
        args.outbound_target = VPS_8GB_OUTPEERS;
    }
    if !cli_set("maxinpeers") && cfg.inbound_limit == stock.inbound_limit {
        args.inbound_limit = VPS_8GB_MAXINPEERS;
    }
    if !cli_set("rpcmaxclients") && cfg.rpc_max_clients == stock.rpc_max_clients {
        args.rpc_max_clients = VPS_8GB_RPCMAXCLIENTS;
    }
    if !cli_set("nogrpc") && cfg.disable_grpc == stock.disable_grpc {
        args.disable_grpc = true;
    }
    if !cli_set("min-disk-free-percent") && cfg.min_disk_free_percent == stock.min_disk_free_percent {
        args.min_disk_free_percent = VPS_8GB_MIN_DISK_FREE_PERCENT;
    }
}

use clap::parser::ValueSource::DefaultValue;
use std::marker::{Send, Sync};
fn arg_match_unwrap_or<T: Clone + Send + Sync + 'static>(m: &clap::ArgMatches, arg_id: &str, default: T) -> T {
    m.get_one::<T>(arg_id).cloned().filter(|_| m.value_source(arg_id) != Some(DefaultValue)).unwrap_or(default)
}

fn arg_match_many_unwrap_or<T: Clone + Send + Sync + 'static>(m: &clap::ArgMatches, arg_id: &str, default: Vec<T>) -> Vec<T> {
    match m.get_many::<T>(arg_id) {
        Some(val_ref) => val_ref.cloned().collect(),
        None => default,
    }
}

/*

  -V, --version                             Display version information and exit
  -C, --configfile=                         Path to configuration file (default: /Users/aspect/Library/Application
                                            Support/Kaspad/kaspad.conf)
  -b, --appdir=                             Directory to store data (default: /Users/aspect/Library/Application
                                            Support/Kaspad)
      --logdir=                             Directory to log output.
  -a, --addpeer=                            Add a peer to connect with at startup
      --connect=                            Connect only to the specified peers at startup
      --nolisten                            Disable listening for incoming connections -- NOTE: Listening is
                                            automatically disabled if the --connect or --proxy options are used
                                            without also specifying listen interfaces via --listen
      --listen=                             Add an interface/port to listen for connections (default all interfaces
                                            port: 26111, testnet: 26211)
      --outpeers=                           Target number of outbound peers (default: 8)
      --maxinpeers=                         Max number of inbound peers (default: 117)
      --enablebanning                       Enable banning of misbehaving peers
      --banduration=                        How long to ban misbehaving peers. Valid time units are {s, m, h}. Minimum
                                            1 second (default: 24h0m0s)
      --banthreshold=                       Maximum allowed ban score before disconnecting and banning misbehaving
                                            peers. (default: 100)
      --whitelist=                          Add an IP network or IP that will not be banned. (eg. 192.168.1.0/24 or
                                            ::1)
      --rpclisten=                          Add an interface/port to listen for RPC connections (default port: 26110,
                                            testnet: 26210)
      --rpccert=                            File containing the certificate file (default:
                                            /Users/aspect/Library/Application Support/Kaspad/rpc.cert)
      --rpckey=                             File containing the certificate key (default:
                                            /Users/aspect/Library/Application Support/Kaspad/rpc.key)
      --rpcmaxclients=                      Max number of RPC clients for standard connections (default: 128)
      --rpcmaxwebsockets=                   Max number of RPC websocket connections (default: 25)
      --rpcmaxconcurrentreqs=               Max number of concurrent RPC requests that may be processed concurrently
                                            (default: 20)
      --norpc                               Disable built-in RPC server
      --saferpc                             Disable RPC commands which affect the state of the node
      --nodnsseed                           Disable DNS seeding for peers
      --dnsseed=                            Override DNS seeds with specified hostname (Only 1 hostname allowed)
      --grpcseed=                           Hostname of gRPC server for seeding peers
      --externalip=                         Add an ip to the list of local addresses we claim to listen on to peers
      --proxy=                              Connect via SOCKS5 proxy (eg. 127.0.0.1:9050)
      --proxyuser=                          Username for proxy server
      --proxypass=                          Password for proxy server
      --dbtype=                             Database backend to use for the Block DAG
      --profile=                            Enable HTTP profiling on given port -- NOTE port must be between 1024 and
                                            65536
  -d, --loglevel=                           Logging level for all subsystems {trace, debug, info, warn, error,
                                            critical} -- You may also specify
                                            <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for
                                            individual subsystems -- Use show to list available subsystems (default:
                                            info)
      --upnp                                Use UPnP to map our listening port outside of NAT
      --minrelaytxfee=                      The minimum transaction fee in KAS/kB to be considered a non-zero fee.
                                            (default: 1e-05)
      --maxorphantx=                        Max number of orphan transactions to keep in memory (default: 100)
      --blockmaxmass=                       Maximum transaction mass to be used when creating a block (default:
                                            10000000)
      --uacomment=                          Comment to add to the user agent -- See BIP 14 for more information.
      --nopeerbloomfilters                  Disable bloom filtering support
      --sigcachemaxsize=                    The maximum number of entries in the signature verification cache
                                            (default: 100000)
      --blocksonly                          Do not accept transactions from remote peers.
      --relaynonstd                         Relay non-standard transactions regardless of the default settings for the
                                            active network.
      --rejectnonstd                        Reject non-standard transactions regardless of the default settings for
                                            the active network.
      --reset-db                            Reset database before starting node. It's needed when switching between
                                            subnetworks.
      --maxutxocachesize=                   Max size of loaded UTXO into ram from the disk in bytes (default:
                                            5000000000)
      --utxoindex                           Enable the UTXO index
      --archival                            Run as an archival node: don't delete old block data when moving the
                                            pruning point (Warning: heavy disk usage)'
      --protocol-version=                   Use non default p2p protocol version (default: 5)
      --enable-unsynced-mining              Allow the node to accept blocks from RPC while not synced
                                            (required when initiating a new network from genesis)
      --testnet                             Use the test network
      --simnet                              Use the simulation test network
      --devnet                              Use the development test network
      --override-dag-params-file=           Overrides DAG params (allowed only on devnet)
  -s, --service=                            Service command {install, remove, start, stop}
      --nogrpc                              Don't initialize the gRPC server
*/

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn parse(extra: &[&str]) -> Args {
        let mut argv = vec!["kaspad"];
        argv.extend_from_slice(extra);
        Args::parse(argv).expect("args parse")
    }

    #[test]
    fn profile_local_validator_enables_borsh_only() {
        let a = parse(&["--profile=local-validator"]);
        assert!(matches!(a.rpclisten_borsh, Some(WrpcNetAddress::Default)));
        assert!(a.rpclisten_json.is_none());
        assert!(a.evm_rpc_listen.is_none());
    }

    #[test]
    fn profile_local_full_enables_borsh_json_evm_loopback() {
        let a = parse(&["--profile=local-full"]);
        assert!(matches!(a.rpclisten_borsh, Some(WrpcNetAddress::Default)));
        assert!(matches!(a.rpclisten_json, Some(WrpcNetAddress::Default)));
        assert!(a.evm_rpc_listen.is_some());
    }

    #[test]
    fn profile_public_node_rpc_binds_json_public() {
        let a = parse(&["--profile=public-node-rpc"]);
        assert!(matches!(a.rpclisten_json, Some(WrpcNetAddress::Public)));
    }

    #[test]
    fn explicit_listener_overrides_profile() {
        // An explicit --rpclisten-borsh wins over the profile's loopback default.
        let a = parse(&["--profile=local-full", "--rpclisten-borsh=public"]);
        assert!(matches!(a.rpclisten_borsh, Some(WrpcNetAddress::Public)));
        // json/evm still come from the profile.
        assert!(matches!(a.rpclisten_json, Some(WrpcNetAddress::Default)));
    }

    #[test]
    fn no_profile_leaves_listeners_unset() {
        let a = parse(&[]);
        assert!(a.profile.is_none());
        assert!(a.rpclisten_borsh.is_none());
        assert!(a.rpclisten_json.is_none());
    }

    #[test]
    fn default_node_profile_is_full_and_noop() {
        let default = Args::default();
        let a = parse(&[]);
        assert_eq!(a.node_profile, NodeProfile::Full);
        assert!(!a.vps_8gb);
        assert_eq!(a.ram_scale, default.ram_scale);
        assert_eq!(a.outbound_target, default.outbound_target);
        assert_eq!(a.inbound_limit, default.inbound_limit);
        assert_eq!(a.rpc_max_clients, default.rpc_max_clients);
        assert_eq!(a.min_disk_free_percent, 0);
    }

    #[test]
    fn bootstrap_pruned_applies_8gb_resource_defaults() {
        let a = parse(&["--node-profile=bootstrap-pruned"]);
        assert_eq!(a.node_profile, NodeProfile::BootstrapPruned);
        assert_eq!(a.ram_scale, VPS_8GB_RAM_SCALE);
        assert_eq!(a.async_threads, VPS_8GB_ASYNC_THREADS.min(num_cpus::get().max(1)));
        assert_eq!(a.outbound_target, VPS_8GB_OUTPEERS);
        assert_eq!(a.inbound_limit, VPS_8GB_MAXINPEERS);
        assert_eq!(a.rpc_max_clients, VPS_8GB_RPCMAXCLIENTS);
        assert!(a.disable_grpc);
        assert_eq!(a.min_disk_free_percent, VPS_8GB_MIN_DISK_FREE_PERCENT);
    }

    #[test]
    fn vps_8gb_flag_applies_resource_defaults_without_sync_only_profile() {
        let a = parse(&["--vps-8gb"]);
        assert_eq!(a.node_profile, NodeProfile::Full);
        assert!(a.vps_8gb);
        assert_eq!(a.ram_scale, VPS_8GB_RAM_SCALE);
        assert_eq!(a.outbound_target, VPS_8GB_OUTPEERS);
        assert_eq!(a.inbound_limit, VPS_8GB_MAXINPEERS);
        assert_eq!(a.rpc_max_clients, VPS_8GB_RPCMAXCLIENTS);
        assert!(a.disable_grpc);
        assert_eq!(a.min_disk_free_percent, VPS_8GB_MIN_DISK_FREE_PERCENT);
    }

    #[test]
    fn explicit_cli_values_override_node_profile_defaults() {
        let a =
            parse(&["--node-profile=bootstrap-pruned", "--ram-scale=0.5", "--outpeers=16", "--min-disk-free-percent=7", "--nogrpc"]);
        assert_eq!(a.ram_scale, 0.5);
        assert_eq!(a.outbound_target, 16);
        assert_eq!(a.min_disk_free_percent, 7);
        assert!(a.disable_grpc);
        assert_eq!(a.inbound_limit, VPS_8GB_MAXINPEERS);
    }

    #[test]
    fn recovery_sync_parses_with_connect() {
        let a = parse(&["--node-profile=recovery-sync", "--connect=1.2.3.4:26111"]);
        assert_eq!(a.node_profile, NodeProfile::RecoverySync);
        assert_eq!(a.connect_peers.len(), 1);
    }

    #[test]
    fn archive_profile_is_label_only() {
        let a = parse(&["--node-profile=archive"]);
        assert_eq!(a.node_profile, NodeProfile::Archive);
        assert_eq!(a.ram_scale, Args::default().ram_scale);
        assert!(!a.disable_grpc);
    }
}
