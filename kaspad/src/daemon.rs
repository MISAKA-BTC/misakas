use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::exit,
    sync::Arc,
    time::Duration,
};

use async_channel::unbounded;
use kaspa_consensus_core::{
    config::ConfigBuilder,
    constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
    errors::config::{ConfigError, ConfigResult},
    mining_rules::MiningRules,
    palw_pruned_frontier::validate_palw_pruning_snapshot_checkpoints,
};
use kaspa_consensus_notify::{root::ConsensusNotificationRoot, service::NotifyService};
use kaspa_core::{core::Core, debug, info, warn};
use kaspa_core::{kaspad_env::version, task::tick::TickService};
use kaspa_database::{
    prelude::{CachePolicy, DbWriter, DirectDbWriter, RocksDbPreset},
    registry::DatabaseStorePrefixes,
};
use kaspa_grpc_server::service::GrpcService;
use kaspa_notify::{address::tracker::Tracker, subscription::context::SubscriptionContext};
use kaspa_p2p_lib::Hub;
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use kaspa_rpc_service::service::{RpcCoreService, ValidatorStatusProvider};
use kaspa_txscript::caches::TxScriptCacheCounters;
use kaspa_utils::git;
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_utils::sysinfo::SystemInfo;
use kaspa_utils_tower::counters::TowerConnectionCounters;

use kaspa_addressmanager::AddressManager;
use kaspa_consensus::{
    consensus::factory::MultiConsensusManagementStore, model::stores::headers::DbHeadersStore, pipeline::monitor::ConsensusMonitor,
};
use kaspa_consensus::{
    consensus::factory::{Factory as ConsensusFactory, LATEST_DB_VERSION},
    params::{OverrideParams, Params},
    pipeline::ProcessingCounters,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::runtime::AsyncRuntime;
use kaspa_index_processor::service::IndexService;
use kaspa_mining::{
    MiningCounters,
    manager::{MiningManager, MiningManagerProxy},
    monitor::MiningMonitor,
};
use kaspa_p2p_flows::{flow_context::FlowContext, service::P2pService};

use kaspa_perf_monitor::{builder::Builder as PerfMonitorBuilder, counters::CountersSnapshot};
use kaspa_utxoindex::{UtxoIndex, api::UtxoIndexProxy};
use kaspa_wrpc_server::service::{Options as WrpcServerOptions, WebSocketCounters as WrpcServerCounters, WrpcEncoding, WrpcService};

/// Desired soft FD limit that needs to be configured
/// for the kaspad process.
pub const DESIRED_DAEMON_SOFT_FD_LIMIT: u64 = 8 * 1024;
/// Minimum acceptable soft FD limit for the kaspad
/// process. (Rusty Kaspa will operate with the minimal
/// acceptable limit of `4096`, but a setting below
/// this value may impact the database performance).
pub const MINIMUM_DAEMON_SOFT_FD_LIMIT: u64 = 4 * 1024;

/// If set, the retention period days must be at least this value
/// (otherwise it is meaningless since pruning periods are typically at least 2 days long)
const MINIMUM_RETENTION_PERIOD_DAYS: f64 = 2.0;
const ONE_GIGABYTE: f64 = 1_000_000_000.0;

use crate::args::{Args, NodeProfile, VPS_8GB_MIN_SYSTEM_MEMORY_BYTES, palw_permissionless_snapshot_auth_refusal};
use crate::disk_guard::{DiskGuard, DiskGuardThresholds, DiskPressureHandle, data_mount_free_percent};
use crate::evm_retention::EvmRetentionService;
use crate::palw_da_spool::{PalwDaSpoolConfig, PalwDaSpoolService};
use crate::palw_mine_service::{PalwMineConfig, PalwMineService, PreparedPalwMineConfig};
use crate::validator_service::{ValidatorConfig, ValidatorMode, ValidatorService};

pub(crate) const DEFAULT_DATA_DIR: &str = "datadir";
pub(crate) const CONSENSUS_DB: &str = "consensus";
const UTXOINDEX_DB: &str = "utxoindex";
const META_DB: &str = "meta";
const META_DB_FILE_LIMIT: i32 = 5;
const DEFAULT_LOG_DIR: &str = "logs";

/// Resolve the ordinary `--connect` policy versus the closed-PALW policy.
///
/// Historically `--connect` means client-only: explicit permanent outbound requests, no address-
/// manager outbound peers, and no listener. Applying that unchanged to a network which requires
/// `--connect` on *every* node makes a two-node network impossible because nobody listens. PALW keeps
/// peer discovery disabled (`outbound_target = 0`) but retains the configured inbound capacity; the
/// P2P adaptor separately enforces the explicit peers as a fail-closed inbound IP allowlist.
fn explicit_peer_connection_limits(
    has_connect_peers: bool,
    palw_allowlisted_listener: bool,
    configured_outbound: usize,
    configured_inbound: usize,
) -> (usize, usize) {
    if !has_connect_peers {
        (configured_outbound, configured_inbound)
    } else if palw_allowlisted_listener {
        (0, configured_inbound)
    } else {
        (0, 0)
    }
}

fn get_home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return dirs::data_local_dir().unwrap();
    #[cfg(not(target_os = "windows"))]
    return dirs::home_dir().unwrap();
}

/// Get the default application directory.
pub fn get_app_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return get_home_dir().join("rusty-kaspa");
    #[cfg(not(target_os = "windows"))]
    return get_home_dir().join(".rusty-kaspa");
}

pub fn validate_args(args: &Args) -> ConfigResult<()> {
    #[cfg(feature = "devnet-prealloc")]
    {
        if args.num_prealloc_utxos.is_some() && !(args.devnet || args.simnet) {
            return Err(ConfigError::PreallocUtxosOnNonDevnet);
        }

        if args.prealloc_address.is_some() ^ args.num_prealloc_utxos.is_some() {
            return Err(ConfigError::MissingPreallocNumOrAddress);
        }
    }

    if !args.connect_peers.is_empty() && !args.add_peers.is_empty() {
        return Err(ConfigError::MixedConnectAndAddPeers);
    }
    if args.logdir.is_some() && args.no_log_files {
        return Err(ConfigError::MixedLogDirAndNoLogFiles);
    }
    if args.ram_scale < 0.1 {
        return Err(ConfigError::RamScaleTooLow);
    }
    if args.ram_scale > 10.0 {
        return Err(ConfigError::RamScaleTooHigh);
    }
    if args.max_tracked_addresses > Tracker::MAX_ADDRESS_UPPER_BOUND {
        return Err(ConfigError::MaxTrackedAddressesTooHigh(Tracker::MAX_ADDRESS_UPPER_BOUND));
    }
    if args.min_disk_free_percent > 99 {
        return Err(ConfigError::MinDiskFreePercentTooHigh(args.min_disk_free_percent));
    }
    // C-01 storage chain: fail closed on a partial chain instead of demoting it to a
    // no-op deep in the virtual processor. Checked on the FINAL values, so it also
    // covers the config-file and environment paths, not just the CLI.
    if args.evm_retire_206 && !args.evm_flat_authoritative {
        return Err(ConfigError::EvmStorageKnobRequires("--evm-retire-206", "--evm-flat-authoritative"));
    }
    if args.evm_flat_authoritative && !args.evm_shadow_state_backend {
        return Err(ConfigError::EvmStorageKnobRequires("--evm-flat-authoritative", "--evm-shadow-state-backend"));
    }
    if args.evm_prune_legacy_206 && !args.evm_retire_206 {
        return Err(ConfigError::EvmStorageKnobRequires("--evm-prune-legacy-206", "--evm-retire-206"));
    }
    if args.node_profile.is_sync_only() {
        let profile = args.node_profile.as_str().to_string();
        if args.archival {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--archival"));
        }
        if args.utxoindex {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--utxoindex"));
        }
        if args.enable_validator {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--enable-validator"));
        }
        if args.enable_beacon {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--enable-beacon"));
        }
        if args.palw_mine {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--palw-mine"));
        }
        if args.palw_da_import_dir.is_some() {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--palw-da-import-dir"));
        }
        // ADR-0042: a sync-only profile is pruned by definition, and a chain-derived boundary is
        // authenticated by the below-pruning-point anti-spam support-row headers that pruning deletes.
        // Named explicitly rather than left to the `--archival` arm above, so an operator who sets the
        // lever WITHOUT --archival is told which flag is the problem instead of neither firing.
        if args.palw_permissionless_snapshot_auth {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--palw-permissionless-snapshot-auth"));
        }
        if args.evm_rpc_listen.is_some() {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--evm-rpc-listen"));
        }
        if args.unsafe_rpc {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--unsaferpc"));
        }
    }
    // kaspa-pq ADR-0040 (AUTH-03 / C-1): a PALW miner needs BOTH the ticket-authority signing key and
    // the ticket-secret store. Missing either produces a node that draws tickets it can never mint —
    // one has no signature for clause 7, the other cannot open its own leaf's nullifier commitment —
    // and a PALW ticket gets one draw per interval with no re-roll, so the loss is not recoverable by
    // retrying. Fail at startup rather than at the first win.
    if args.palw_mine {
        if args.palw_ticket_authority_key.is_none() {
            return Err(ConfigError::PalwMineRequiresTicketAuthorityKey);
        }
        if args.palw_ticket_secret_file.is_none() {
            return Err(ConfigError::PalwMineRequiresTicketSecretFile);
        }
        if args.palw_mine_address.is_none() {
            return Err(ConfigError::PalwMineRequiresAddress);
        }
        if args.palw_leaf.is_empty() {
            return Err(ConfigError::PalwMineRequiresLeaf);
        }
        if !args.palw_enable_algo4 {
            return Err(ConfigError::PalwMineRequiresAlgo4Acceptance);
        }
    }
    if args.palw_da_import_dir.is_some() && !args.palw_enable_algo4 {
        return Err(ConfigError::PalwDaImportRequiresAlgo4Acceptance);
    }
    validate_palw_pruning_snapshot_checkpoints(&args.palw_pruning_snapshot_checkpoints)
        .map_err(|err| ConfigError::InvalidPalwPruningSnapshotCheckpoints(err.to_string()))?;
    if matches!(args.node_profile, NodeProfile::RecoverySync) && args.connect_peers.is_empty() {
        return Err(ConfigError::RecoverySyncRequiresConnect);
    }
    Ok(())
}

/// Log the EFFECTIVE EVM storage configuration as one line at startup.
///
/// The failure this prevents is not exotic: a node was believed to be retiring the
/// per-block 206 state snapshot while the dependency chain had demoted that knob to a
/// no-op, and the only evidence was a warning line among thousands. One unconditional
/// line naming the profile and whether the per-block full-state copy is still being
/// written makes the storage behaviour greppable in `journalctl` after the fact.
fn log_evm_storage_profile(args: &Args) {
    if !cfg!(feature = "evm") {
        // Every EVM knob is inert in a non-EVM build; do not imply otherwise.
        return;
    }
    let named = args.evm_storage_profile.map(|p| p.as_str()).unwrap_or("(unset — individual knobs)");
    info!(
        "EVM storage profile: {} — history={} shadow-state-backend={} flat-authoritative={} retire-206={}",
        named,
        args.evm_history_mode.as_str(),
        args.evm_shadow_state_backend,
        args.evm_flat_authoritative,
        args.evm_retire_206,
    );
    if !args.evm_retire_206 {
        warn!(
            "EVM storage: the per-block 206 full-state snapshot IS being persisted — EVM state storage grows as \
             O(state x kept blocks), and pruning is deferred while consensus is transitional or catching up, so it can \
             outrun reclamation during IBD. Use --evm-storage-profile=compact to bound it at O(state)."
        );
    }
}

fn disk_free_preflight(args: &Args, app_dir: &Path) {
    if args.min_disk_free_percent == 0 {
        return;
    }

    match data_mount_free_percent(app_dir) {
        Some(free) if free < args.min_disk_free_percent as f64 => {
            println!(
                "Refusing to start kaspad: free disk on the data mount ({}) is {:.1}% < required {}%. \
                 Free space or lower --min-disk-free-percent.",
                app_dir.display(),
                free,
                args.min_disk_free_percent
            );
            exit(1);
        }
        Some(free) => {
            info!("Disk preflight: {:.1}% free on the data mount (>= {}% required)", free, args.min_disk_free_percent);
        }
        None => {
            warn!(
                "Disk preflight: could not determine free space for {}; skipping the >= {}% check",
                app_dir.display(),
                args.min_disk_free_percent
            );
        }
    }
}

fn request_database_deletion_approval(approve: bool) -> bool {
    let msg = "Node database is from a different Kaspad *DB* version and needs to be fully deleted, do you confirm the delete? (y/n)";
    get_user_approval_or_exit(msg, approve);
    info!("Deleting databases from previous Kaspad version");
    true // if consensus not exited, always return true
}
fn get_user_approval_or_exit(message: &str, approve: bool) {
    if approve {
        return;
    }
    println!("{}", message);
    let mut input = String::new();
    match std::io::stdin().read_line(&mut input) {
        Ok(_) => {
            let lower = input.to_lowercase();
            let answer = lower.as_str().strip_suffix("\r\n").or(lower.as_str().strip_suffix('\n')).unwrap_or(lower.as_str());
            if answer == "y" || answer == "yes" {
                // return
            } else {
                println!("Operation was rejected ({}), exiting..", answer);
                exit(1);
            }
        }
        Err(error) => {
            println!("Error reading from console: {error}, exiting..");
            exit(1);
        }
    }
}

/// Runtime configuration struct for the application.
#[derive(Default)]
pub struct Runtime {
    log_dir: Option<String>,
}

/// Get the application directory from the supplied [`Args`].
/// This function can be used to identify the location of
/// the application folder that contains kaspad logs and the database.
pub fn get_app_dir_from_args(args: &Args) -> PathBuf {
    let app_dir = args
        .appdir
        .clone()
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) }
}

/// Get the log directory from the supplied [`Args`].
pub fn get_log_dir(args: &Args) -> Option<String> {
    let network = args.network();
    let app_dir = get_app_dir_from_args(args);

    // Logs directory is usually under the application directory, unless otherwise specified
    let log_dir = args.logdir.clone().unwrap_or_default().replace('~', get_home_dir().as_path().to_str().unwrap());
    let log_dir = if log_dir.is_empty() { app_dir.join(network.to_prefixed()).join(DEFAULT_LOG_DIR) } else { PathBuf::from(log_dir) };

    if args.no_log_files { None } else { log_dir.to_str().map(String::from) }
}

impl Runtime {
    pub fn from_args(args: &Args) -> Self {
        let log_dir = get_log_dir(args);

        // Initialize the logger
        cfg_if::cfg_if! {
            if #[cfg(feature = "semaphore-trace")] {
                kaspa_core::log::init_logger(log_dir.as_deref(), &format!("{},{}=debug", args.log_level, kaspa_utils::sync::semaphore_module_path()));
            } else {
                kaspa_core::log::init_logger(log_dir.as_deref(), &args.log_level);
            }
        };

        // Configure the panic behavior
        // As we log the panic, we want to set it up after the logger
        kaspa_core::panic::configure_panic();

        Self { log_dir: log_dir.map(|log_dir| log_dir.to_owned()) }
    }
}

/// Audit (2026-06-26): whether the operator has explicitly acknowledged exposing the
/// unauthenticated, CORS-open EVM JSON-RPC adapter on a public (non-loopback) address. Mirrors
/// the bridge dashboard's `RKSTRATUM_ALLOW_PUBLIC_DASHBOARD` gate — without it, a non-loopback
/// `--evm-rpc-listen` is refused at startup.
#[cfg(feature = "evm")]
fn public_evm_rpc_allowed() -> bool {
    matches!(std::env::var("MISAKA_ALLOW_PUBLIC_EVM_RPC").as_deref(), Ok("1") | Ok("true") | Ok("TRUE"))
}

/// Create [`Core`] instance with supplied [`Args`].
/// This function will automatically create a [`Runtime`]
/// instance with the supplied [`Args`] and then
/// call [`create_core_with_runtime`].
///
/// Usage semantics:
/// `let (core, rpc_core_service) = create_core(args);`
///
/// The instance of the [`RpcCoreService`] needs to be released
/// (dropped) before the `Core` is shut down.
///
pub fn create_core(args: Args, fd_total_budget: i32) -> (Arc<Core>, Arc<RpcCoreService>) {
    let rt = Runtime::from_args(&args);
    create_core_with_runtime(&rt, &args, fd_total_budget)
}

/// Configure RocksDB parameters from CLI arguments.
///
/// Returns: (preset, cache_budget, wal_directory)
fn configure_rocksdb(args: &Args) -> (RocksDbPreset, Option<usize>, Option<PathBuf>) {
    // Parse preset
    let preset = if let Some(preset_str) = &args.rocksdb_preset {
        match preset_str.parse::<RocksDbPreset>() {
            Ok(p) => {
                info!("Using RocksDB preset: {} - {}", p, p.description());
                info!("  Use case: {}", p.use_case());
                info!("  Memory requirements: {}", p.memory_requirements());
                p
            }
            Err(err) => {
                println!("Invalid RocksDB preset: {}", err);
                exit(1);
            }
        }
    } else {
        RocksDbPreset::Default
    };

    // Calculate cache budget for HDD preset
    let cache_budget = if matches!(preset, RocksDbPreset::Hdd) {
        if let Some(cache_mb) = args.rocksdb_cache_size {
            let cache_bytes = cache_mb * 1024 * 1024;
            info!("Custom RocksDB cache size: {} MB", cache_mb);
            Some(cache_bytes)
        } else {
            let base_cache = 256 * 1024 * 1024;
            let scaled_cache = (base_cache as f64 * args.ram_scale) as usize;
            let min_cache = 64 * 1024 * 1024;
            let final_cache = scaled_cache.max(min_cache);
            info!("RocksDB cache size: {} MB (scaled by ram-scale)", final_cache / 1024 / 1024);
            Some(final_cache)
        }
    } else {
        None
    };

    // Setup WAL directory if specified
    let wal_dir = args.rocksdb_wal_dir.as_ref().map(|custom_wal_dir| {
        let wal_path = PathBuf::from(custom_wal_dir);
        info!("Custom WAL directory: {}", wal_path.display());
        wal_path
    });

    (preset, cache_budget, wal_dir)
}

/// Create [`Core`] instance with supplied [`Args`] and [`Runtime`].
///
/// Usage semantics:
/// ```ignore
/// let Runtime = Runtime::from_args(&args); // or create your own
/// let (core, rpc_core_service) = create_core(&runtime, &args);
/// ```
///
/// The instance of the [`RpcCoreService`] needs to be released
/// (dropped) before the `Core` is shut down.
///
pub fn create_core_with_runtime(runtime: &Runtime, args: &Args, fd_total_budget: i32) -> (Arc<Core>, Arc<RpcCoreService>) {
    let network = args.network();
    let mut fd_remaining = fd_total_budget;
    let utxo_files_limit = if args.utxoindex {
        let utxo_files_limit = fd_remaining / 10;
        fd_remaining -= utxo_files_limit;
        utxo_files_limit
    } else {
        0
    };

    // Configure RocksDB parameters
    let (rocksdb_preset, cache_budget, wal_dir) = configure_rocksdb(args);

    // Make sure args forms a valid set of properties
    if let Err(err) = validate_args(args) {
        println!("{}", err);
        exit(1);
    }

    let params = {
        let params: Params = network.into();
        match &args.override_params_file {
            Some(path) => {
                if network.is_mainnet() {
                    println!("Overriding params on mainnet is not allowed.");
                    exit(1);
                }

                let file_content = fs::read_to_string(path).unwrap_or_else(|err| {
                    println!("Failed to read override params file '{}': {}", path, err);
                    exit(1);
                });
                let override_params: OverrideParams = serde_json::from_str(&file_content).unwrap_or_else(|err| {
                    println!("Failed to parse override params file '{}': {}", path, err);
                    exit(1);
                });
                params.override_params(override_params)
            }
            None => params,
        }
    };

    let config = Arc::new(
        ConfigBuilder::new(params).adjust_perf_params_to_consensus_params().apply_args(|config| args.apply_to_config(config)).build(),
    );

    let prepared_palw_da_spool = args.palw_da_import_dir.as_ref().map(|path| {
        if !config.params.palw_algo4_accept {
            println!(
                "Refusing to start: --palw-da-import-dir was supplied but algo-4 acceptance is not active on {}. \
                 This local spool is an Object-v2 publication surface and never degrades to an inert/warning-only mode.",
                config.params.net
            );
            exit(1);
        }
        PalwDaSpoolConfig::prepare(PathBuf::from(path)).unwrap_or_else(|error| {
            println!("Refusing to start: invalid --palw-da-import-dir: {error}");
            exit(1);
        })
    });

    // PALW mining is fail-closed: materialise its payout, authority and ticket store before any daemon
    // service starts. `TicketSecretStore::load_or_empty` is useful to registration code, but accepting
    // an absent file here would turn a typo into an apparently healthy miner which can never open a
    // registered leaf. Keep the prepared, non-optional values until the service is constructed below.
    let prepared_palw_mine_config: Option<PreparedPalwMineConfig> = if args.palw_mine {
        let palw_active = config.params.palw_activation_daa_score != u64::MAX && config.params.dns_params.is_some();
        let prepared = args
            .palw_leaf
            .iter()
            .map(|s| crate::palw_mine_service::parse_leaf_ref(s))
            .collect::<Result<Vec<_>, _>>()
            .map_err(ConfigError::PalwMineInvalidConfiguration)
            .and_then(|owned_leaves| {
                PalwMineConfig {
                    address: args.palw_mine_address.clone(),
                    address_prefix: config.prefix(),
                    palw_active,
                    ticket_authority_key_path: args.palw_ticket_authority_key.clone(),
                    ticket_secret_path: args.palw_ticket_secret_file.clone().map(PathBuf::from),
                    owned_leaves,
                }
                .prepare()
                .map_err(ConfigError::PalwMineInvalidConfiguration)
            });
        match prepared {
            Ok(prepared) => Some(prepared),
            Err(err) => {
                println!("{err}");
                exit(1);
            }
        }
    } else {
        None
    };

    // kaspa-pq **ADR-0040 P1-13 (BIND-04 / SS-01)** — shipped PALW presets must run archival.
    //
    // A bounded, atomic snapshot/import path exists for coordinated Header-v3 networks, but those
    // headers have no first-descendant commitment that can authenticate the imported PALW sidecar.
    // Header-v4 can import only a complete payload digest pinned out of band by the operator; this
    // does not turn any shipped Header-v3 preset into a permissionless pruning network or make a peer
    // digest trusted. The shipped presets therefore retain their archival/closed-network policy.
    if config.params.palw_requires_archival && !config.is_archival {
        println!(
            "Refusing to start: {} requires --archival. Header-v3 PALW snapshot import is \
             closed-network-only; Header-v4 import requires an explicit operator-pinned complete \
             payload digest and does not relax this shipped-preset policy (ADR-0040 P1-13).",
            config.params.net
        );
        exit(1);
    }

    // kaspa-pq **ADR-0040 P1-5** — PALW params-validity PREFLIGHT, enforced rather than asserted.
    //
    // `PalwBatchAdmissionParams::is_consistent_for_activation` and
    // `LaneDifficultyParams::is_consistent_for_activation` both document themselves as the bound that
    // keeps a one-word params edit from silently restoring unbounded pre-cap behaviour — and
    // `PalwParams::is_structurally_valid` says outright that it is "called at config-build time". None
    // of the three had a single production caller: every invocation in the tree was a `#[test]`
    // assertion over a hand-maintained list of presets, so a preset added without being added to that
    // list was checked by nothing. That is the exact defect class this ADR keeps closing — a bound that
    // only a comment enforces is not a bound.
    //
    // This is that enforcement point. It runs only where PALW actually activates, so the inert
    // placeholder values on the non-PALW presets are not required to satisfy it, and it refuses at
    // startup with a reason instead of degrading — the same rule as the archival check above and the
    // seeder's PALW refusal.
    if config.params.palw_activation_daa_score != u64::MAX {
        let failed = if !config.params.palw_batch_admission.is_consistent_for_activation() {
            Some(
                "palw_batch_admission (a zero cap would make the per-block overlay view unbounded; a zero \
                 min_provider_bond_sompi would make splitting a provider bond free)",
            )
        } else if !config.params.palw_lane_difficulty.is_consistent_for_activation(config.params.genesis.bits) {
            Some("palw_lane_difficulty (§16.3 re-genesis preflight: window / target / scale / clamp vs genesis bits)")
        } else {
            None
        };
        if let Some(which) = failed {
            println!(
                "Refusing to start: {} activates the PALW lane (palw_activation_daa_score = {}) but its \
                 {which} params are inconsistent (ADR-0040 P1-5). Fix the preset in \
                 consensus/core/src/config/params.rs — running with them would leave a documented bound \
                 unenforced.",
                config.params.net, config.params.palw_activation_daa_score
            );
            exit(1);
        }
    }

    // kaspa-pq **ADR-0040 §T-shared** — a PALW network must be unreachable by UNLISTED peers, not
    // merely unadvertised.
    //
    // The DNS seeder already refuses these networks, but that only stops them being announced: anyone
    // who knows the netsuffix can still dial in. While the activation gates are unreleased the safety
    // argument rests on no unlisted third party reaching the net, so the node requires an explicit
    // peer allowlist (`--connect`). The P2P server applies those IPs before its handshake; listed nodes
    // can still accept each other, which is necessary for a shared closed network.
    //
    // Enforced here rather than left to a firewall, for the same reason the seeder refuses rather than
    // omits: a closure that depends on someone remembering to configure it is not a closure.
    if config.params.palw_requires_peer_allowlist && args.connect_peers.is_empty() {
        println!(
            "Refusing to start: {} requires an explicit peer allowlist (--connect). PALW activation \
             gates are not released (ADR-0040 §7.1.1), so this network is only safe while unreachable by \
             unlisted third parties — and 'not listed on the seeder' is not the same as 'not reachable'.",
            config.params.net
        );
        exit(1);
    }

    // kaspa-pq **ADR-0042** — the permissionless (chain-derived) pruning-snapshot auth lever,
    // re-checked on the FINAL configuration.
    //
    // `Args::parse` already refuses the unsupported combinations, but this is the authoritative check
    // and the only one that sees the values the node will actually run: params after
    // `--override-params-file`, algo-4 acceptance after `apply_to_config` resolved it, and archival
    // after the profile defaults. It also covers a programmatically built `Args`, which never went
    // through the CLI parser at all.
    //
    // It reads the operator's REQUEST (`args.palw_permissionless_snapshot_auth`) rather than the
    // projected config field on purpose: `Args::apply_to_config` drops the flag on an unsupported
    // network, and a node started with an explicitly requested but silently dropped consensus-import
    // lever must refuse, not run as a no-op that looks enabled in the operator's command line.
    if args.palw_permissionless_snapshot_auth {
        if let Some(reason) =
            palw_permissionless_snapshot_auth_refusal(&config.params, config.is_archival, config.params.palw_algo4_accept)
        {
            println!("Refusing to start: --palw-permissionless-snapshot-auth: {reason}");
            exit(1);
        }
        // Enabled and honoured. Stated unconditionally and in full, because the one thing this lever
        // must never do is quietly imply that some OTHER fence was relaxed: it widens what this node
        // IMPORTS, and leaves the archival requirement and the peer allowlist exactly where they were
        // (both re-asserted here from the effective configuration, not from the flags).
        warn!(
            "PALW chain-derived (permissionless) pruning-snapshot import is ENABLED on {} (ADR-0042, StopShip). \
             Node-local and consensus-neutral: this node will admit a Header-v4 boundary it authenticates from \
             the chain instead of from --palw-pruning-snapshot-checkpoint. It does NOT open the network and does \
             NOT relax any other fence — peer-allowlist-required={}, --connect peers={}, archival={}, \
             algo4-accept={}, operator pins={}. ADR-0042's Definition of Done is not closed (transport \
             integration, the full-lifecycle 1c fixture, the 3 review points, the multi-node v4 soak): do not \
             run this on a public or value-bearing network.",
            config.params.net,
            config.params.palw_requires_peer_allowlist,
            args.connect_peers.len(),
            config.is_archival,
            config.params.palw_algo4_accept,
            config.palw_pruning_snapshot_checkpoints.len(),
        );
    }

    let app_dir = get_app_dir_from_args(args);
    let db_dir = app_dir.join(network.to_prefixed()).join(DEFAULT_DATA_DIR);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), git::with_short_hash(version()));

    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());
    for checkpoint in &config.palw_pruning_snapshot_checkpoints {
        info!("PALW operator-authenticated pruning snapshot checkpoint: {checkpoint}");
    }
    match runtime.log_dir.as_ref() {
        Some(s) => {
            info!("Logs directory: {}", s);
        }
        None => {
            info!("Logs to console only");
        }
    }

    if args.node_profile != NodeProfile::Full || args.vps_8gb {
        info!(
            "Node profile: {} (vps-8gb={}) — ram-scale={}, async-threads={}, outpeers={}, maxinpeers={}, rpcmaxclients={}, nogrpc={}",
            args.node_profile,
            args.vps_8gb,
            config.ram_scale,
            args.async_threads,
            args.outbound_target,
            args.inbound_limit,
            args.rpc_max_clients,
            args.disable_grpc,
        );
    }
    if args.vps_8gb {
        let total_memory = SystemInfo::default().total_memory;
        if total_memory > 0 && total_memory < VPS_8GB_MIN_SYSTEM_MEMORY_BYTES {
            warn!(
                "--vps-8gb is set but total system memory is {:.1} GB (< {:.1} GB recommended). Headroom will be tight.",
                total_memory as f64 / ONE_GIGABYTE,
                VPS_8GB_MIN_SYSTEM_MEMORY_BYTES as f64 / ONE_GIGABYTE,
            );
        }
    }
    log_evm_storage_profile(args);
    disk_free_preflight(args, &app_dir);

    let consensus_db_dir = db_dir.join(CONSENSUS_DB);
    let utxoindex_db_dir = db_dir.join(UTXOINDEX_DB);
    let meta_db_dir = db_dir.join(META_DB);

    let mut is_db_reset_needed = args.reset_db;

    // Reset Condition: User explicitly requested a reset
    if is_db_reset_needed && db_dir.exists() {
        let msg = "Reset DB was requested -- this means the current databases will be fully deleted,
do you confirm? (answer y/n or pass --yes to the Kaspad command line to confirm all interactive questions)";
        get_user_approval_or_exit(msg, args.yes);
        info!("Deleting databases");
        fs::remove_dir_all(&db_dir).unwrap();
    }

    fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
    fs::create_dir_all(meta_db_dir.as_path()).unwrap();
    if args.utxoindex {
        info!("Utxoindex Data directory {}", utxoindex_db_dir.display());
        fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
    }

    if !args.archival
        && let Some(retention_period_days) = args.retention_period_days
    {
        // Look only at post-fork values (which are the worst-case)
        let finality_depth = config.finality_depth();
        let target_time_per_block = config.target_time_per_block(); // in ms

        let retention_period_milliseconds = (retention_period_days * 24.0 * 60.0 * 60.0 * 1000.0).ceil() as u64;
        if MINIMUM_RETENTION_PERIOD_DAYS <= retention_period_days {
            let total_blocks = retention_period_milliseconds / target_time_per_block;
            // This worst case usage only considers block space. It does not account for usage of
            // other stores (reachability, block status, mempool, etc.)
            let worst_case_usage =
                ((total_blocks + finality_depth) * (config.max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR)) as f64 / ONE_GIGABYTE;

            info!(
                "Retention period is set to {} days. Disk usage may be up to {:.2} GB for block space required for this period.",
                retention_period_days, worst_case_usage
            );
        } else {
            panic!("Retention period ({}) must be at least {} days", retention_period_days, MINIMUM_RETENTION_PERIOD_DAYS);
        }
    }

    // DB used for addresses store and for multi-consensus management
    let mut meta_db = kaspa_database::prelude::ConnBuilder::default()
        .with_db_path(meta_db_dir.clone())
        .with_files_limit(META_DB_FILE_LIMIT)
        .with_preset(rocksdb_preset)
        .with_wal_dir(wal_dir.clone())
        .with_cache_budget(cache_budget)
        .build()
        .unwrap();

    // Reset Condition: Need to reset DB if we can't find genesis in current DB
    if !is_db_reset_needed && (args.testnet || args.devnet || args.simnet) {
        // Non-mainnet can be restarted, and when it does we need to reset the DB.
        // This will check if the current Genesis can be found the active consensus
        // DB (if one exists), and if not then ask to reset the DB.
        let active_consensus_dir_name = MultiConsensusManagementStore::new(meta_db.clone()).active_consensus_dir_name().unwrap();

        match active_consensus_dir_name {
            Some(dir_name) => {
                let consensus_db = kaspa_database::prelude::ConnBuilder::default()
                    .with_db_path(consensus_db_dir.clone().join(dir_name))
                    .with_files_limit(1)
                    .with_preset(rocksdb_preset)
                    .with_wal_dir(wal_dir.clone())
                    .with_cache_budget(cache_budget)
                    .build()
                    .unwrap();

                let headers_store = DbHeadersStore::new(consensus_db, CachePolicy::Empty, CachePolicy::Empty);

                if headers_store.has(config.genesis.hash).unwrap() {
                    debug!("Genesis is found in active consensus DB. No action needed.");
                } else {
                    let msg = "Genesis not found in active consensus DB. This happens when Testnets are restarted and your database needs to be fully deleted. Do you confirm the delete? (y/n)";
                    get_user_approval_or_exit(msg, args.yes);

                    is_db_reset_needed = true;
                }
            }
            None => {
                debug!("Consensus not initialized yet. Skipping genesis check.");
            }
        }
    }

    // Reset Condition: Need to reset if we're upgrading from kaspad DB version
    // TEMP: upgrade from Alpha version or any version before this one
    'db_upgrade: while !is_db_reset_needed
        && (meta_db.get_pinned(b"multi-consensus-metadata-key").is_ok_and(|r| r.is_some())
            || MultiConsensusManagementStore::new(meta_db.clone()).should_upgrade().unwrap())
    {
        let mut mcms = MultiConsensusManagementStore::new(meta_db.clone());
        let version = mcms.version().unwrap();

        if version <= 3 {
            is_db_reset_needed = request_database_deletion_approval(args.yes);
            continue 'db_upgrade;
        }

        // kaspa-pq ADR-0039/ADR-0040 PALW (DB version 8 → 9): a HARD reset, not a soft upgrade.
        // The PALW slices changed the positional bincode layout of records that are already on
        // disk — `GhostdagData`/`CompactGhostdagData` gained two mid-struct work fields (written
        // for every block on EVERY preset), and the PALW overlay/certificate rows gained four more
        // (written on `testnet-palw-110`/`devnet-palw-111`, which activate at DAA 0). The 8 → 9 step
        // is the ADR-0040 P1-5 remediation, which REMOVES the trailing `job_nullifiers` map from
        // `PalwBatchViewV1` — a removal breaks a positional encoding exactly as an addition does, and
        // those rows are written on both PALW presets. Old bytes are
        // strictly shorter than the new decoders require, so reading them yields a bincode EOF that
        // surfaces as an `.unwrap()` panic in a pipeline worker. There is no migration: reject the
        // old DB at open time per ADR-0001.
        //
        // Placed BEFORE the "instant and safe" soft-upgrade message below on purpose — that message
        // would be a lie on this path, and reaching the `assert_eq!` at the end of the loop with an
        // un-upgraded version would panic instead of prompting.
        //
        // CONSEQUENCE, deliberate: this subsumes the `version <= 4` and `version <= 5` soft-upgrade
        // arms below. The loop only runs when `version != LATEST_DB_VERSION (15)`, so every older version
        // this binary can encounter is `<= 14` and hard-resets here; those arms are now unreachable.
        // They are retained unmodified as the record of the 4→5→6 migration and as the shape to
        // follow if a future version is ever soft-upgradable. A soft upgrade cannot bridge a
        // positional-encoding break, so there is no version in 4..=11 they could legitimately serve.
        //
        // kaspa-pq ADR-0040 §5.15 raised the bound 8 -> 9. Version 9 is the one case in this range
        // where the old DB is not SHORT but semantically stale: `leaf_root` kept its byte layout and
        // changed its construction, so a v9 datadir would decode cleanly and then fail every leaf
        // membership proof. That is precisely why it must hard-reset rather than soft-upgrade.
        //
        // kaspa-pq ADR-0040 §5.15.13 (gate G16) raised it 9 -> 10. Version 10 is stale for a different
        // reason again: nothing it holds is malformed, but it predates the `PalwPaidWork` column family
        // that the reward-coordinate duplicate-work walk reads. Resuming on it would silently evaluate
        // that rule against an empty history — no decode error, just a wrong reward set. Hard-reset.
        //
        // SUMMARY-BIND V2 raises the DB version 11 -> 12 and this reset bound 10 -> 11. Version 11
        // certificates persist the legacy auditor-vote element layout; V2 inserts two signed summary
        // fields before each signature. Those positional rows cannot be migrated or decoded safely.
        //
        // DA-01 raises 12 -> 13 and the reset bound 11 -> 12. Prefixes 250-252 introduce fork-local
        // obligations/challenges, canonical DA objects and the pruning snapshot. Starting them empty on
        // a v12 history would bypass certificate/reward/exit enforcement for pre-upgrade work.
        //
        // PALW pruning blobs raise 13 -> 14 and this reset bound 12 -> 13. The v14 singleton carries
        // the active manifest/leaf/certificate projection required for fresh first-post-PP validation;
        // reusing a v13 singleton would either fail positional decode or silently lack those blobs.
        //
        // Search-availability dispatch raises 14 -> 15 and this reset bound 13 -> 14. Prefixes
        // 189-191 add per-block search obligation rows every active chain block must carry (a v14
        // history fail-stops the selected-parent loader and the reorg registry reconciler on the
        // first missing row), the pruning payload gains a positional field, and
        // `PalwSelectedParentStateV2` gains the search state root that moves every Header-v4
        // overlay commitment. Hard reset per ADR-0001.
        if version <= 14 {
            is_db_reset_needed = request_database_deletion_approval(args.yes);
            continue 'db_upgrade;
        }

        let msg = "NOTE: Node database is from an older version. Proceeding with the upgrade is instant and safe.
However, downgrading to an older node version later will require deleting the database.
Do you confirm? (y/n)";
        get_user_approval_or_exit(msg, args.yes);
        if version <= 4 {
            mcms.set_version(5).unwrap();
        }
        if version <= 5 {
            let active_consensus_dir_name = mcms.active_consensus_dir_name().unwrap();

            match active_consensus_dir_name {
                Some(current_consensus_db) => {
                    // Apply soft upgrade logic: delete relation data from higher levels
                    // and then update DB version to 6

                    let consensus_db = kaspa_database::prelude::ConnBuilder::default()
                        .with_db_path(consensus_db_dir.clone().join(current_consensus_db))
                        .with_files_limit(10)
                        .with_preset(rocksdb_preset)
                        .with_wal_dir(wal_dir.clone())
                        .with_cache_budget(cache_budget)
                        .build()
                        .unwrap();

                    let start_level: u8 = 1;
                    let start_level_bytes = start_level.to_le_bytes();

                    let mut writer = DirectDbWriter::new(&consensus_db);

                    let end_level: u8 = config.max_block_level + 1;
                    let end_level_bytes = end_level.to_le_bytes();

                    let start_parents_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsParents.into_iter().chain(start_level_bytes).collect();
                    let end_parents_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsParents.into_iter().chain(end_level_bytes).collect();

                    let start_children_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsChildren.into_iter().chain(start_level_bytes).collect();
                    let end_children_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsChildren.into_iter().chain(end_level_bytes).collect();

                    // Apply delete of range from level 1 to max (+1) for RelationsParents and RelationsChildren:
                    writer.delete_range(start_parents_prefix_vec.clone(), end_parents_prefix_vec.clone()).unwrap();
                    writer.delete_range(start_children_prefix_vec.clone(), end_children_prefix_vec.clone()).unwrap();

                    //  update the version to one higher:
                    mcms.set_version(6).unwrap();
                    info!("Deprecated stores have been removed from database, storage will be gradually cleared in due time.");
                    info!("Database is now in version 6");
                }
                None => {
                    is_db_reset_needed = request_database_deletion_approval(args.yes);
                    continue 'db_upgrade;
                }
            }
        }
        // if we reached here, db should be upgraded fully and we should exit the loop next
        assert_eq!(mcms.version().unwrap(), LATEST_DB_VERSION);
    }

    // Will be true if any of the other condition above except args.reset_db
    // has set is_db_reset_needed to true
    if is_db_reset_needed && !args.reset_db {
        // Drop so that deletion works
        drop(meta_db);

        // Delete
        fs::remove_dir_all(db_dir.clone()).unwrap();

        // Recreate the empty folders
        fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
        fs::create_dir_all(meta_db_dir.as_path()).unwrap();

        if args.utxoindex {
            fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
        }

        // Reopen the DB
        meta_db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(meta_db_dir)
            .with_files_limit(META_DB_FILE_LIMIT)
            .with_preset(rocksdb_preset)
            .with_wal_dir(wal_dir.clone())
            .with_cache_budget(cache_budget)
            .build()
            .unwrap();
    }

    if !args.archival && MultiConsensusManagementStore::new(meta_db.clone()).is_archival_node().unwrap() {
        get_user_approval_or_exit(
            "--archival is set to false although the node was previously archival. Proceeding may delete archived data. Do you confirm? (y/n)",
            args.yes,
        );
    }

    let connect_peers = args.connect_peers.iter().map(|x| x.normalize(config.default_p2p_port())).collect::<Vec<_>>();
    let add_peers = args.add_peers.iter().map(|x| x.normalize(config.default_p2p_port())).collect();
    let p2p_server_addr = args.listen.unwrap_or(ContextualNetAddress::unspecified()).normalize(config.default_p2p_port());
    // Ordinary `--connect` remains client-only. Closed PALW networks are the deliberate exception:
    // every node is required to name explicit peers, so at least one of them must still listen. The
    // listener is not public: ConnectionHandler rejects a remote IP unless it appears here before it
    // constructs a Router or runs the handshake. Ports are ignored because inbound source ports are
    // ephemeral; the configured address is still used verbatim for the permanent outbound request.
    let palw_allowlisted_listener = config.params.palw_requires_peer_allowlist && !connect_peers.is_empty();
    let inbound_ip_allowlist = palw_allowlisted_listener.then(|| connect_peers.iter().map(|peer| peer.ip.0).collect::<HashSet<_>>());
    let (outbound_target, inbound_limit) = explicit_peer_connection_limits(
        !connect_peers.is_empty(),
        palw_allowlisted_listener,
        args.outbound_target,
        args.inbound_limit,
    );
    let dns_seeders = if connect_peers.is_empty() && !args.disable_dns_seeding { config.dns_seeders } else { &[] };

    // P2P bootstrap mode (design §9): make it explicit how peers are discovered and WHY
    // DNS seeding was or wasn't used — so an operator who passed --connect/--addpeer/
    // --nodnsseed understands why the seed path was skipped.
    if !connect_peers.is_empty() {
        info!(
            "P2P bootstrap: explicit --connect peers only ({}) — DNS seed and peer discovery disabled{}",
            connect_peers.len(),
            if palw_allowlisted_listener { "; inbound listener restricted to their IPs" } else { "" }
        );
    } else if !dns_seeders.is_empty() {
        info!(
            "P2P bootstrap: DNS seed ({} seeder(s)){}",
            dns_seeders.len(),
            if args.add_peers.is_empty() { "" } else { " + explicit --addpeer peers" }
        );
    } else if args.disable_dns_seeding {
        info!("P2P bootstrap: DNS seed disabled (--nodnsseed) — using explicit peers + the address manager");
    } else {
        info!("P2P bootstrap: no DNS seeders configured for this network — using explicit peers + the address manager");
    }

    let grpc_server_addr = args.rpclisten.unwrap_or(ContextualNetAddress::loopback()).normalize(config.default_rpc_port());

    // Captured before `config` is moved into the consensus factory.
    let evm_retention_interval_ms = config.evm_retention_policy.interval_ms;

    let core = Arc::new(Core::new());

    // ---

    let tick_service = Arc::new(TickService::new());
    // Runtime disk guard. `--min-disk-free-percent` only ever gated STARTUP, so a node
    // that filled its mount while running had no warning, no growth rate, and no clean
    // stop. The handle stays Normal when the guard is disabled, so consumers need no
    // conditional path.
    let disk_pressure = DiskPressureHandle::new();
    let (notification_send, notification_recv) = unbounded();
    let max_tracked_addresses = if args.utxoindex && args.max_tracked_addresses > 0 { Some(args.max_tracked_addresses) } else { None };
    let subscription_context = SubscriptionContext::with_options(max_tracked_addresses);
    let notification_root = Arc::new(ConsensusNotificationRoot::with_context(notification_send, subscription_context.clone()));
    let processing_counters = Arc::new(ProcessingCounters::default());
    let mining_counters = Arc::new(MiningCounters::default());
    let wrpc_borsh_counters = Arc::new(WrpcServerCounters::default());
    let wrpc_json_counters = Arc::new(WrpcServerCounters::default());
    let tx_script_cache_counters = Arc::new(TxScriptCacheCounters::default());
    let p2p_tower_counters = Arc::new(TowerConnectionCounters::default());
    let grpc_tower_counters = Arc::new(TowerConnectionCounters::default());

    // Use `num_cpus` background threads for the consensus database as recommended by rocksdb
    let mining_rules = Arc::new(MiningRules::default());
    let consensus_db_parallelism = num_cpus::get();
    let consensus_factory = Arc::new(ConsensusFactory::new(
        meta_db.clone(),
        &config,
        consensus_db_dir,
        consensus_db_parallelism,
        notification_root.clone(),
        processing_counters.clone(),
        tx_script_cache_counters.clone(),
        fd_remaining,
        mining_rules.clone(),
        rocksdb_preset,
        wal_dir.clone(),
        cache_budget,
    ));
    let consensus_manager = Arc::new(ConsensusManager::new(consensus_factory));
    let consensus_monitor = Arc::new(ConsensusMonitor::new(processing_counters.clone(), tick_service.clone()));

    let perf_monitor_builder = PerfMonitorBuilder::new()
        .with_fetch_interval(Duration::from_secs(args.perf_metrics_interval_sec))
        .with_tick_service(tick_service.clone());
    let perf_monitor = if args.perf_metrics {
        let cb = move |counters: CountersSnapshot| {
            debug!("[{}] {}", kaspa_perf_monitor::SERVICE_NAME, counters.to_process_metrics_display());
            debug!("[{}] {}", kaspa_perf_monitor::SERVICE_NAME, counters.to_io_metrics_display());
            #[cfg(feature = "heap")]
            debug!("[{}] heap stats: {:?}", kaspa_perf_monitor::SERVICE_NAME, dhat::HeapStats::get());
        };
        Arc::new(perf_monitor_builder.with_fetch_cb(cb).build())
    } else {
        Arc::new(perf_monitor_builder.build())
    };

    let system_info = SystemInfo::default();

    let notify_service = Arc::new(NotifyService::new(notification_root.clone(), notification_recv, subscription_context.clone()));
    // §9 (evm builds): the Ethereum RPC newHeads pump registers a
    // VirtualChainChanged listener on the consensus notifier — capture a handle now,
    // before notify_service is moved into the async runtime below.
    #[cfg(feature = "evm")]
    let eth_consensus_notifier = notify_service.notifier();
    let index_service: Option<Arc<IndexService>> = if args.utxoindex {
        // Use only a single thread for none-consensus databases
        let utxoindex_db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(utxoindex_db_dir)
            .with_files_limit(utxo_files_limit)
            .with_preset(rocksdb_preset)
            .with_wal_dir(wal_dir.clone())
            .with_cache_budget(cache_budget)
            .build()
            .unwrap();
        let utxoindex = UtxoIndexProxy::new(UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap());
        let index_service = Arc::new(IndexService::new(&notify_service.notifier(), subscription_context.clone(), Some(utxoindex)));
        Some(index_service)
    } else {
        None
    };

    let (address_manager, port_mapping_extender_svc) = AddressManager::new(config.clone(), meta_db, tick_service.clone());

    // kaspa-pq EVM Lane v0.4 (§8.2/§16): the miner's declared EVM coinbase.
    // A malformed value is a startup error (silently burning a miner's priority
    // fees to the zero address would be worse).
    let evm_fee_recipient = args.evm_fee_recipient.as_ref().map(|s| {
        let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
        let mut bytes = [0u8; 20];
        if h.len() != 40 || faster_hex::hex_decode(h.as_bytes(), &mut bytes).is_err() {
            panic!("--evm-fee-recipient must be a 20-byte hex address (40 hex chars, optional 0x): got {s}");
        }
        kaspa_consensus_core::evm::EvmAddress::from_bytes(bytes)
    });
    // kaspa-pq DNS-finality: local attestation mempool/mining policy (expiry, dedup, recent-epoch
    // template preference). Enabled and configured from the chain's `DnsParams` when present
    // (mainnet/testnet/devnet/simnet carry it); disabled otherwise so behavior is unchanged.
    let attestation_policy = match config.dns_params.as_ref() {
        Some(dns_params) => kaspa_mining::AttestationMempoolPolicy::from_dns_params(dns_params),
        None => kaspa_mining::AttestationMempoolPolicy::disabled(),
    };
    let mining_manager = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
        config.target_time_per_block(),
        false,
        config.max_block_mass,
        config.ram_scale,
        config.block_template_cache_lifetime,
        mining_counters.clone(),
        // kaspa-pq PQ-only relay: every kaspa-pq network enforces PQ at genesis, so the production
        // mempool requires ML-DSA-87 P2PKH outputs/inputs (audit Finding C).
        true,
        evm_fee_recipient,
        attestation_policy,
    )));
    let mining_monitor =
        Arc::new(MiningMonitor::new(mining_manager.clone(), mining_counters, tx_script_cache_counters.clone(), tick_service.clone()));

    let hub = Hub::new();
    let mining_rule_engine = Arc::new(MiningRuleEngine::new(
        consensus_manager.clone(),
        config.clone(),
        processing_counters.clone(),
        tick_service.clone(),
        hub.clone(),
        mining_rules,
    ));
    let flow_context = Arc::new(FlowContext::new(
        consensus_manager.clone(),
        address_manager,
        config.clone(),
        mining_manager.clone(),
        tick_service.clone(),
        notification_root,
        hub.clone(),
        mining_rule_engine.clone(),
    ));
    let palw_da_spool_service =
        prepared_palw_da_spool.map(|config| Arc::new(PalwDaSpoolService::new(config, tick_service.clone(), flow_context.clone())));

    // kaspa-pq Phase 11 (ADR-0010) + ADR-0039 Phase 6: in-process DNS-overlay validator service,
    // optionally also submitting PALW beacon commit/reveal txs (`--enable-beacon`). Built when either
    // is set (so default node behavior is unchanged) and after `flow_context`, which it uses to submit
    // attestation-shard + beacon transactions.
    let validator_service = if args.enable_validator || args.enable_beacon {
        let mode = match args.validator_mode.as_deref() {
            Some(s) => s.parse::<ValidatorMode>().unwrap_or_else(|err| {
                warn!("{err}; falling back to observer mode");
                ValidatorMode::default()
            }),
            None => ValidatorMode::default(),
        };
        // Equivocation-safety log lives beside the per-network data dir (NOT inside it),
        // so it survives a `--reset-db` and still binds the validator to its network.
        let state_path = app_dir.join(network.to_prefixed()).join("validator-state.json");
        // kaspa-pq ADR-0039 Phase 6: the beacon only runs on a PALW-active net (testnet-palw /
        // devnet-palw). Its liveness ALSO needs the DNS-health leg (attestations), so warn if
        // --enable-beacon was set without --enable-validator.
        let palw_active = config.params.palw_activation_daa_score != u64::MAX && config.params.dns_params.is_some();
        let enable_beacon = args.enable_beacon && palw_active;
        if args.enable_beacon && !palw_active {
            warn!(
                "--enable-beacon: the PALW lane is inactive on this network; the beacon is a no-op here (use testnet-palw / devnet-palw)."
            );
        }
        if args.enable_beacon && !args.enable_validator {
            warn!(
                "--enable-beacon without --enable-validator: the beacon reaches quorum only when DNS is healthy, which needs this node's attestations — also pass --enable-validator --validator-mode active."
            );
        }
        let beacon_secret_path = app_dir.join(network.to_prefixed()).join("beacon-secret.json");
        let validator_config = ValidatorConfig {
            mode,
            key_path: args.validator_key.clone(),
            stake_bond: args.stake_bond.clone(),
            state_path: Some(state_path),
            address_prefix: config.prefix(),
            enable_beacon,
            palw_network_id: config.params.net.suffix().unwrap_or(0),
            palw_epoch_length_daa: config.params.palw_epoch_length_daa,
            beacon_secret_path: Some(beacon_secret_path),
        };
        let validator_mass_calculator = kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&config.params);
        Some(Arc::new(ValidatorService::new(
            validator_config,
            consensus_manager.clone(),
            tick_service.clone(),
            flow_context.clone(),
            validator_mass_calculator,
            index_service.as_ref().map(|x| x.utxoindex().unwrap()),
            config.params.coinbase_maturity(),
            disk_pressure.clone(),
        )))
    } else {
        None
    };

    // kaspa-pq ADR-0039 Phase 5: in-process PALW algo-4 mining service. Built only when `--palw-mine`
    // is set (default node behavior unchanged) and, like the validator, AFTER `flow_context` (which it
    // uses to submit the minted algo-4 block) and BEFORE `flow_context` is moved into RpcCoreService.
    // All fallible configuration was resolved by the startup preflight above, so this service cannot
    // exist in a warning-only, permanently inert state.
    let palw_mine_service = prepared_palw_mine_config.map(|palw_mine_config| {
        Arc::new(PalwMineService::new(palw_mine_config, consensus_manager.clone(), tick_service.clone(), flow_context.clone()))
    });

    let p2p_service = Arc::new(P2pService::new(
        flow_context.clone(),
        connect_peers,
        add_peers,
        p2p_server_addr,
        outbound_target,
        inbound_limit,
        inbound_ip_allowlist,
        dns_seeders,
        config.default_p2p_port(),
        p2p_tower_counters.clone(),
    ));

    // kaspa-pq Phase 11 (ADR-0010): expose the in-process validator service's status via
    // the `getValidatorStatus` RPC (None when `--enable-validator` is off).
    let validator_status_provider: Option<Arc<dyn ValidatorStatusProvider>> = match &validator_service {
        Some(v) => Some(v.clone()),
        None => None,
    };
    // kaspa-pq EVM Lane (ADR-0020 §16): keep a mining-manager handle for the
    // Ethereum JSON-RPC adapter (`eth_sendRawTransaction`). Routed through the
    // `flow_context` (admit + P2P-broadcast to EVM-relay peers, like the UTXO RPC
    // submit), cloned here because `flow_context` is moved into the RPC core
    // service just below.
    #[cfg(feature = "evm")]
    let flow_context_for_eth = flow_context.clone();
    let rpc_core_service = Arc::new(RpcCoreService::new(
        consensus_manager.clone(),
        notify_service.notifier(),
        index_service.as_ref().map(|x| x.notifier()),
        mining_manager,
        flow_context,
        subscription_context,
        index_service.as_ref().map(|x| x.utxoindex().unwrap()),
        config.clone(),
        core.clone(),
        processing_counters,
        wrpc_borsh_counters.clone(),
        wrpc_json_counters.clone(),
        perf_monitor.clone(),
        p2p_tower_counters.clone(),
        grpc_tower_counters.clone(),
        system_info,
        mining_rule_engine.clone(),
        validator_status_provider,
    ));
    let grpc_service_broadcasters: usize = 3; // TODO: add a command line argument or derive from other arg/config/host-related fields
    let grpc_service = if !args.disable_grpc {
        Some(Arc::new(GrpcService::new(
            grpc_server_addr,
            config,
            rpc_core_service.clone(),
            args.rpc_max_clients,
            grpc_service_broadcasters,
            grpc_tower_counters,
        )))
    } else {
        None
    };

    // Create an async runtime and register the top-level async services
    let async_runtime = Arc::new(AsyncRuntime::new(args.async_threads));
    async_runtime.register(tick_service.clone());
    async_runtime.register(notify_service);
    if let Some(index_service) = index_service {
        async_runtime.register(index_service)
    };
    if let Some(port_mapping_extender_svc) = port_mapping_extender_svc {
        async_runtime.register(Arc::new(port_mapping_extender_svc))
    };
    async_runtime.register(rpc_core_service.clone());
    if let Some(grpc_service) = grpc_service {
        async_runtime.register(grpc_service)
    }
    async_runtime.register(p2p_service);
    async_runtime.register(consensus_monitor);
    if let Some(validator_service) = validator_service {
        async_runtime.register(validator_service)
    };
    if let Some(palw_mine_service) = palw_mine_service {
        async_runtime.register(palw_mine_service)
    };
    if let Some(palw_da_spool_service) = palw_da_spool_service {
        async_runtime.register(palw_da_spool_service)
    };
    // kaspa-pq EVM Lane (ADR-0020 §16): the Ethereum JSON-RPC adapter, enabled by
    // `--evm-rpc-listen` (evm builds only; the default node never links it).
    #[cfg(feature = "evm")]
    if let Some(evm_rpc_listen) = &args.evm_rpc_listen {
        let addr: std::net::SocketAddr = evm_rpc_listen.normalize(8545).into();
        // Audit (2026-06-26) High: the adapter is UNAUTHENTICATED and CORS-open
        // (Access-Control-Allow-Origin: *). A non-loopback bind is now FAIL-CLOSED — refuse to
        // start unless the operator explicitly acknowledges the risk, mirroring the bridge
        // dashboard's public-bind gate (a WARN is not enough; a stray 0.0.0.0 must not silently
        // expose a public unauthenticated JSON-RPC).
        if !addr.ip().is_loopback() && !public_evm_rpc_allowed() {
            println!(
                "Refusing to start: --evm-rpc-listen is bound to a NON-LOOPBACK address ({addr}), but the \
                 Ethereum JSON-RPC adapter is UNAUTHENTICATED and CORS-open. Bind it to 127.0.0.1 and front it \
                 with a TLS + auth + rate-limiting reverse proxy, or set MISAKA_ALLOW_PUBLIC_EVM_RPC=1 to \
                 acknowledge the risk."
            );
            exit(1);
        }
        kaspa_core::info!("Ethereum JSON-RPC adapter enabled on http://{addr}");
        if !addr.ip().is_loopback() {
            kaspa_core::warn!(
                "Ethereum JSON-RPC is bound to a NON-LOOPBACK address ({addr}); it is UNAUTHENTICATED and \
                 CORS-open. MISAKA_ALLOW_PUBLIC_EVM_RPC is set — ensure a TLS + auth + rate-limiting reverse \
                 proxy and a firewall are in front."
            );
        }
        async_runtime.register(Arc::new(crate::eth_rpc::EthRpcService::new(
            addr,
            consensus_manager.clone(),
            flow_context_for_eth,
            eth_consensus_notifier,
        )));
    }
    async_runtime.register(mining_monitor);
    async_runtime.register(perf_monitor);
    async_runtime.register(mining_rule_engine);
    // EVM retention runs on its own clock, NOT behind the L1 pruning gate — that
    // gate stands down for the whole of IBD, which is when EVM data grows fastest.
    async_runtime.register(Arc::new(EvmRetentionService::new(
        tick_service.clone(),
        consensus_manager.clone(),
        Duration::from_millis(evm_retention_interval_ms),
        disk_pressure.clone(),
    )));
    if let Some(thresholds) = DiskGuardThresholds::from_min_free_percent(args.min_disk_free_percent) {
        async_runtime.register(Arc::new(DiskGuard::new(
            tick_service.clone(),
            app_dir.clone(),
            db_dir.join(CONSENSUS_DB),
            thresholds,
            disk_pressure.clone(),
            &core,
        )));
    } else {
        info!("Runtime disk guard is DISABLED (--min-disk-free-percent=0). The node will not warn or stop as the mount fills.");
    }

    let wrpc_service_tasks: usize = 2; // num_cpus::get() / 2;
    // Register wRPC servers based on command line arguments
    [
        (args.rpclisten_borsh.clone(), WrpcEncoding::Borsh, wrpc_borsh_counters),
        (args.rpclisten_json.clone(), WrpcEncoding::SerdeJson, wrpc_json_counters),
    ]
    .into_iter()
    .filter_map(|(listen_address, encoding, wrpc_server_counters)| {
        listen_address.map(|listen_address| {
            Arc::new(WrpcService::new(
                wrpc_service_tasks,
                Some(rpc_core_service.clone()),
                &encoding,
                wrpc_server_counters,
                WrpcServerOptions {
                    listen_address: listen_address.to_address(&network.network_type, &encoding).to_string(), // TODO: use a normalized ContextualNetAddress instead of a String
                    verbose: args.wrpc_verbose,
                    ..WrpcServerOptions::default()
                },
            ))
        })
    })
    .for_each(|server| async_runtime.register(server));

    // Endpoint summary (design §9): print exactly what each transport binds, in
    // protocol-clear names, so operators never confuse node gRPC / wRPC Borsh /
    // wRPC JSON / EVM JSON-RPC / P2P. Disabled transports are shown as such.
    {
        let nt = network.network_type;
        let borsh = args.rpclisten_borsh.as_ref().map(|a| a.to_address(&nt, &WrpcEncoding::Borsh).to_string());
        let json = args.rpclisten_json.as_ref().map(|a| a.to_address(&nt, &WrpcEncoding::SerdeJson).to_string());
        #[cfg(feature = "evm")]
        let evm = args.evm_rpc_listen.as_ref().map(|a| a.normalize(8545).to_string());
        #[cfg(not(feature = "evm"))]
        let evm: Option<String> = None;
        let show = |o: &Option<String>| o.clone().unwrap_or_else(|| "disabled".to_string());
        info!("MISAKA node endpoints (network {network}):");
        info!("  P2P:             {p2p_server_addr}   node-to-node only (not RPC)");
        info!("  node-grpc:       {grpc_server_addr}   miner / low-level node RPC");
        info!("  node-wrpc-borsh: {}   validator / wallet / operator", show(&borsh));
        info!("  node-wrpc-json:  {}   explorer / browser", show(&json));
        info!("  evm-rpc-http:    {}   Ethereum JSON-RPC (EVM lane)", show(&evm));

        // Public-bind security warning (design §7.1/§15.5). Warn (not refuse — that
        // would break existing public deployments) when an RPC listener binds a
        // non-loopback address without the --allow-public-rpc acknowledgement.
        if !args.allow_public_rpc {
            let is_public = |s: &str| !(s.contains("127.0.0.1") || s.contains("[::1]") || s.starts_with("localhost"));
            let grpc = grpc_server_addr.to_string();
            let mut public: Vec<String> = Vec::new();
            if is_public(&grpc) {
                public.push(format!("node-grpc {grpc}"));
            }
            if let Some(b) = borsh.as_deref().filter(|s| is_public(s)) {
                public.push(format!("node-wrpc-borsh {b}"));
            }
            if let Some(j) = json.as_deref().filter(|s| is_public(s)) {
                public.push(format!("node-wrpc-json {j}"));
            }
            if !public.is_empty() {
                warn!(
                    "SECURITY: node RPC bound to a non-loopback address ({}) without --allow-public-rpc. \
                     Front it with a TLS + auth + rate-limiting reverse proxy, or restrict the bind to 127.0.0.1. \
                     Pass --allow-public-rpc to acknowledge and silence this warning.",
                    public.join(", ")
                );
            }
        }

        // Endpoint registry (design §7): record the loopback RPC endpoints this node
        // bound to `~/.misaka/<network-id>/endpoints.json`, so the miner / validator /
        // unified CLI can auto-discover them and the operator never types a port. The
        // host is normalized to 127.0.0.1 (a co-located reader connects over loopback);
        // a registry write failure is non-fatal (just a missing convenience).
        let loopback = |addr: &str| addr.rsplit_once(':').map(|(_, p)| format!("127.0.0.1:{p}"));
        let registry = misaka_endpoints::EndpointRegistry::new(
            &network.to_string(),
            misaka_endpoints::Endpoints {
                node_grpc: loopback(&grpc_server_addr.to_string()),
                node_wrpc_borsh: borsh.as_deref().and_then(loopback),
                node_wrpc_json: json.as_deref().and_then(loopback),
                evm_rpc_http: evm.as_deref().and_then(loopback),
            },
            args.profile.clone(),
        );
        if let Err(e) = registry.write() {
            warn!("could not write endpoint registry (non-fatal): {e}");
        }
    }

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus_manager);
    core.bind(async_runtime);

    (core, rpc_core_service)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra: &[&str]) -> Args {
        let mut argv = vec!["kaspad"];
        argv.extend_from_slice(extra);
        Args::parse(argv).expect("args parse")
    }

    #[test]
    fn bootstrap_pruned_rejects_utxoindex() {
        let args = parse(&["--node-profile=bootstrap-pruned", "--utxoindex"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::NodeProfileIncompatible(_, "--utxoindex"))));
    }

    #[test]
    fn bootstrap_pruned_rejects_archival() {
        let args = parse(&["--node-profile=bootstrap-pruned", "--archival"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::NodeProfileIncompatible(_, "--archival"))));
    }

    #[test]
    fn bootstrap_pruned_rejects_enable_validator() {
        let args = parse(&["--node-profile=bootstrap-pruned", "--enable-validator"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::NodeProfileIncompatible(_, "--enable-validator"))));
    }

    #[test]
    fn bootstrap_pruned_rejects_evm_rpc_listen() {
        let args = parse(&["--node-profile=bootstrap-pruned", "--evm-rpc-listen=0.0.0.0:8545"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::NodeProfileIncompatible(_, "--evm-rpc-listen"))));
    }

    #[test]
    fn bootstrap_pruned_rejects_unsaferpc() {
        let args = parse(&["--node-profile=bootstrap-pruned", "--unsaferpc"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::NodeProfileIncompatible(_, "--unsaferpc"))));
    }

    #[test]
    fn recovery_sync_requires_connect() {
        let args = parse(&["--node-profile=recovery-sync"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::RecoverySyncRequiresConnect)));
    }

    #[test]
    fn recovery_sync_with_connect_is_valid() {
        let args = parse(&["--node-profile=recovery-sync", "--connect=1.2.3.4:26111"]);
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn min_disk_free_percent_out_of_range_is_rejected() {
        let args = parse(&["--min-disk-free-percent=150"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::MinDiskFreePercentTooHigh(150))));
    }

    #[test]
    fn full_profile_allows_heavy_flags() {
        let args = parse(&["--utxoindex", "--archival", "--enable-validator", "--evm-rpc-listen=127.0.0.1:8545"]);
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn palw_da_spool_is_explicit_and_requires_acceptance() {
        let without_acceptance = parse(&["--palw-da-import-dir=/tmp/palw-da"]);
        assert!(matches!(validate_args(&without_acceptance), Err(ConfigError::PalwDaImportRequiresAlgo4Acceptance)));
        assert!(validate_args(&parse(&["--palw-enable-algo4", "--palw-da-import-dir=/tmp/palw-da"])).is_ok());
        assert!(matches!(
            validate_args(&parse(&["--node-profile=bootstrap-pruned", "--palw-enable-algo4", "--palw-da-import-dir=/tmp/palw-da",])),
            Err(ConfigError::NodeProfileIncompatible(_, "--palw-da-import-dir"))
        ));
    }

    /// ADR-0042: a sync-only profile prunes the below-pruning-point anti-spam support-row headers a
    /// chain-derived boundary is authenticated against, so the two are refused as incompatible with a
    /// message that names THIS flag — not the `--archival` arm the operator did not set.
    #[test]
    fn sync_only_profiles_reject_permissionless_snapshot_auth() {
        for profile in [NodeProfile::BootstrapPruned, NodeProfile::RecoverySync] {
            let args = Args {
                node_profile: profile,
                connect_peers: vec!["1.2.3.4:26111".parse().expect("peer")],
                palw_permissionless_snapshot_auth: true,
                ..Default::default()
            };
            assert!(
                matches!(validate_args(&args), Err(ConfigError::NodeProfileIncompatible(_, "--palw-permissionless-snapshot-auth"))),
                "{}",
                profile.as_str()
            );
        }
    }

    /// The daemon's startup refusal runs on the FINAL params — after `--override-params-file` and
    /// after `apply_to_config` resolved algo-4 acceptance — so a programmatic `Args` that never went
    /// through the CLI parser is fenced by exactly the same predicate.
    #[test]
    fn daemon_refuses_permissionless_snapshot_auth_on_a_non_v4_or_pruned_node() {
        let mut args = Args { palw_permissionless_snapshot_auth: true, archival: true, palw_enable_algo4: true, ..Default::default() };

        // Default network (mainnet, Header-v3, inert anti-spam): refused.
        let mut mainnet = kaspa_consensus_core::config::Config::new(args.network().into());
        args.apply_to_config(&mut mainnet);
        let refusal =
            palw_permissionless_snapshot_auth_refusal(&mainnet.params, mainnet.is_archival, mainnet.params.palw_algo4_accept)
                .expect("mainnet must be refused");
        assert!(refusal.contains("INERT"), "{refusal}");
        // ...and the projection did not leave the lever set behind the refusal.
        assert!(!mainnet.palw_permissionless_snapshot_auth);

        // The staging Header-v4 re-genesis with the full posture: accepted.
        args.testnet = true;
        args.testnet_suffix = 200;
        let mut staging = kaspa_consensus_core::config::Config::new(args.network().into());
        args.apply_to_config(&mut staging);
        assert!(
            palw_permissionless_snapshot_auth_refusal(&staging.params, staging.is_archival, staging.params.palw_algo4_accept)
                .is_none()
        );
        assert!(staging.palw_permissionless_snapshot_auth);

        // Same network, pruned: refused, and the request is still visible on `args` so the daemon
        // refuses to start rather than running a silently demoted no-op.
        args.archival = false;
        let mut pruned = kaspa_consensus_core::config::Config::new(args.network().into());
        args.apply_to_config(&mut pruned);
        assert!(
            palw_permissionless_snapshot_auth_refusal(&pruned.params, pruned.is_archival, pruned.params.palw_algo4_accept)
                .expect("pruned must be refused")
                .contains("--archival")
        );
        assert!(!pruned.palw_permissionless_snapshot_auth);
        assert!(args.palw_permissionless_snapshot_auth);
    }

    /// The lever must never look like it relaxed a neighbouring fence: enabling it leaves the
    /// archival policy, the peer allowlist and algo-4 acceptance exactly where they were.
    #[test]
    fn permissionless_snapshot_auth_does_not_relax_the_archival_or_allowlist_fences() {
        let args =
            parse(&["--testnet", "--netsuffix=200", "--archival", "--palw-enable-algo4", "--palw-permissionless-snapshot-auth"]);
        let baseline = parse(&["--testnet", "--netsuffix=200", "--archival", "--palw-enable-algo4"]);

        let mut with_lever = kaspa_consensus_core::config::Config::new(args.network().into());
        args.apply_to_config(&mut with_lever);
        let mut without_lever = kaspa_consensus_core::config::Config::new(baseline.network().into());
        baseline.apply_to_config(&mut without_lever);

        assert!(with_lever.palw_permissionless_snapshot_auth);
        assert!(!without_lever.palw_permissionless_snapshot_auth);
        // Everything the startup fences read is identical with and without the lever.
        assert_eq!(with_lever.params.palw_requires_archival, without_lever.params.palw_requires_archival);
        assert_eq!(with_lever.params.palw_requires_peer_allowlist, without_lever.params.palw_requires_peer_allowlist);
        assert!(with_lever.params.palw_requires_peer_allowlist, "the shipped v4 preset stays allowlist-closed");
        assert_eq!(with_lever.is_archival, without_lever.is_archival);
        assert_eq!(with_lever.params.palw_algo4_accept, without_lever.params.palw_algo4_accept);
        assert_eq!(args.connect_peers, baseline.connect_peers);
    }

    #[test]
    fn daemon_revalidates_programmatic_palw_snapshot_checkpoint_duplicates() {
        let mut args = Args::default();
        let checkpoint = kaspa_consensus_core::palw_pruned_frontier::PalwPruningSnapshotCheckpoint {
            pruning_point: kaspa_consensus_core::Hash64::from_u64_word(1),
            payload_digest: kaspa_consensus_core::Hash64::from_u64_word(2),
        };
        args.palw_pruning_snapshot_checkpoints = vec![checkpoint, checkpoint];
        assert!(matches!(validate_args(&args), Err(ConfigError::InvalidPalwPruningSnapshotCheckpoints(_))));
    }

    #[test]
    fn operator_snapshot_checkpoint_is_node_local_and_does_not_activate_algo4() {
        let checkpoint =
            format!("{}:{}", kaspa_consensus_core::Hash64::from_u64_word(3), kaspa_consensus_core::Hash64::from_u64_word(4));
        let args = parse(&[&format!("--palw-pruning-snapshot-checkpoint={checkpoint}")]);
        assert!(validate_args(&args).is_ok());
        let params: Params = args.network().into();
        let mut config = kaspa_consensus_core::config::Config::new(params);
        let before = config.params.palw_algo4_accept;
        args.apply_to_config(&mut config);
        assert_eq!(config.params.palw_algo4_accept, before);
        assert_eq!(config.palw_pruning_snapshot_checkpoints.len(), 1);
    }

    #[test]
    fn palw_mining_refuses_inert_empty_configuration() {
        assert!(matches!(validate_args(&parse(&["--palw-mine"])), Err(ConfigError::PalwMineRequiresTicketAuthorityKey)));
        assert!(matches!(
            validate_args(&parse(&["--palw-mine", "--palw-ticket-authority-key-file=authority.seed"])),
            Err(ConfigError::PalwMineRequiresTicketSecretFile)
        ));
        assert!(matches!(
            validate_args(&parse(&[
                "--palw-mine",
                "--palw-ticket-authority-key-file=authority.seed",
                "--palw-ticket-secret-file=tickets.json",
            ])),
            Err(ConfigError::PalwMineRequiresAddress)
        ));
        assert!(matches!(
            validate_args(&parse(&[
                "--palw-mine",
                "--palw-ticket-authority-key-file=authority.seed",
                "--palw-ticket-secret-file=tickets.json",
                "--palw-mine-address=misakatest:qplaceholder",
            ])),
            Err(ConfigError::PalwMineRequiresLeaf)
        ));
        let leaf = format!("--palw-leaf={}:0", "00".repeat(64));
        assert!(matches!(
            validate_args(&parse(&[
                "--palw-mine",
                "--palw-ticket-authority-key-file=authority.seed",
                "--palw-ticket-secret-file=tickets.json",
                "--palw-mine-address=misakatest:qplaceholder",
                leaf.as_str(),
            ])),
            Err(ConfigError::PalwMineRequiresAlgo4Acceptance)
        ));
    }

    #[test]
    fn ordinary_connect_stays_client_only_but_palw_connect_keeps_an_allowlisted_listener() {
        assert_eq!(explicit_peer_connection_limits(false, false, 8, 128), (8, 128));
        assert_eq!(explicit_peer_connection_limits(true, false, 8, 128), (0, 0));
        assert_eq!(explicit_peer_connection_limits(true, true, 8, 128), (0, 128));
        // Operators may still deliberately make one PALW node client-only. The important invariant is
        // that `--connect` no longer forces *every* PALW node to zero inbound capacity.
        assert_eq!(explicit_peer_connection_limits(true, true, 8, 0), (0, 0));
    }

    #[test]
    fn partial_evm_storage_chain_is_rejected_instead_of_silently_demoted() {
        // Each of these used to start fine and log a warning while continuing to write a
        // full EVM state copy per block — the configuration that filled the disk.
        let args = parse(&["--evm-retire-206"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::EvmStorageKnobRequires("--evm-retire-206", _))));

        let args = parse(&["--evm-retire-206", "--evm-flat-authoritative"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::EvmStorageKnobRequires("--evm-flat-authoritative", _))));

        let args = parse(&["--evm-prune-legacy-206"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::EvmStorageKnobRequires("--evm-prune-legacy-206", _))));
    }

    #[test]
    fn compact_storage_profile_passes_validation() {
        let args = parse(&["--evm-storage-profile=compact"]);
        assert!(validate_args(&args).is_ok());
        // And the irreversible reclamation is accepted on top of it (it is the only
        // combination in which it is allowed to run).
        let args = parse(&["--evm-storage-profile=compact", "--evm-prune-legacy-206"]);
        assert!(validate_args(&args).is_ok());
    }
}
