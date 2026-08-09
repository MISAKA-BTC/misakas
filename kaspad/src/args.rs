use clap::{Arg, ArgAction, Command, arg};
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

    // kaspa-pq Phase 11 (ADR-0010): in-process DNS-overlay validator service. Default off.
    pub enable_validator: bool,
    pub validator_key: Option<String>,
    /// kaspa-pq EVM Lane v0.4 (§8.2/§16): the miner's EVM coinbase (20-byte hex,
    /// optional 0x) — claims the priority fees of this node's own payload txs.
    pub evm_fee_recipient: Option<String>,
    pub stake_bond: Option<String>,
    pub validator_mode: Option<String>,

    // MISAKA Verified LLM Token-Weighted BFT: the compute role. Default off, and additionally
    // inert on any network whose model cost table is empty (which is every shipped preset).
    pub enable_compute: bool,
    pub compute_worker: Option<String>,
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
            enable_validator: false,
            validator_key: None,
            evm_fee_recipient: None,
            stake_bond: None,
            validator_mode: None,
            enable_compute: false,
            compute_worker: None,
            compute_work_dir: None,
            compute_prompt: None,
            compute_max_tokens: None,
            compute_timeout_secs: None,
            compute_auto_challenge: false,
            compute_fixture_job_limit: None,
            vlt_devnet_shadow_daa: None,
            vlt_devnet_credit_window_epochs: 8,
            vlt_shadow_only: false,
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
            let dns = config.params.dns_params.take().expect("devnet/simnet ship with the DNS overlay configured").with_vlt_devnet(
                shadow_daa,
                self.vlt_devnet_credit_window_epochs,
                self.vlt_shadow_only,
                genesis_hash,
            );
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
        }

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
        .arg(arg!(--"enable-validator" "kaspa-pq: run the in-process DNS-overlay validator service (ADR-0010). Default off.").env("KASPAD_ENABLE_VALIDATOR"))
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
            arg!(--"vlt-shadow-only" "MISAKA VLT Shadow Mode: with --vlt-devnet, leave the WEIGHT fence dormant. The overlay \
                 runs and is policed for real — certificates credited, committees drawn, verdicts paid, challenges slashing — \
                 while DNS finality stays on bonded stake indefinitely. This is the mode to run before committing to a \
                 weight fence: it produces the C_i(E) you need to see before deciding it is safe to vote on.")
                .env("KASPAD_VLT_SHADOW_ONLY"),
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
        Err(err) => {
            println!("{err}");
            std::process::exit(1);
        }
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
            enable_validator: arg_match_unwrap_or::<bool>(&m, "enable-validator", defaults.enable_validator),
            validator_key: m.get_one::<String>("validator-key").cloned().or(defaults.validator_key),
            evm_fee_recipient: m.get_one::<String>("evm-fee-recipient").cloned().or(defaults.evm_fee_recipient),
            stake_bond: m.get_one::<String>("stake-bond").cloned().or(defaults.stake_bond),
            validator_mode: m.get_one::<String>("validator-mode").cloned().or(defaults.validator_mode),
            enable_compute: arg_match_unwrap_or::<bool>(&m, "enable-compute", defaults.enable_compute),
            compute_worker: m.get_one::<String>("compute-worker").cloned().or(defaults.compute_worker),
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
