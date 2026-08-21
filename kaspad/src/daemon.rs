use std::{
    fs,
    path::{Path, PathBuf},
    process::exit,
    sync::Arc,
    time::Duration,
};

use crate::chain_participation_store::ChainParticipationStore;
use async_channel::unbounded;
use kaspa_consensus_core::{
    config::ConfigBuilder,
    constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
    errors::config::{ConfigError, ConfigResult},
    mining_rules::MiningRules,
};
use kaspa_consensus_notify::{root::ConsensusNotificationRoot, service::NotifyService};
use kaspa_core::{chain_participation::ChainParticipationGate, core::Core, debug, info, warn};
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

use crate::args::{Args, NodeProfile, VPS_8GB_MIN_SYSTEM_MEMORY_BYTES};
use crate::compute::ComputeConfig;
use crate::validator_service::{ValidatorConfig, ValidatorMode, ValidatorService};

/// Default wall-clock ceiling for one compute job. A Qwen3.6-35B-A3B replay at the registered
/// profile's 4096-token limit is minutes, not seconds; the ceiling exists to bound a wedged
/// worker, not to pace a healthy one.
const DEFAULT_COMPUTE_TIMEOUT_SECS: u64 = 900;
/// Default token ceiling this node asks for. Clamped down to the registered profile's own limit,
/// and deliberately well under it: every accepted job is re-executed in full by each verifier, so
/// the ceiling is an audit-cost decision the operator can raise once the mesh has capacity.
const DEFAULT_COMPUTE_MAX_TOKENS: u32 = 512;

const DEFAULT_DATA_DIR: &str = "datadir";
const CONSENSUS_DB: &str = "consensus";
const UTXOINDEX_DB: &str = "utxoindex";
const META_DB: &str = "meta";
const META_DB_FILE_LIMIT: i32 = 5;
const DEFAULT_LOG_DIR: &str = "logs";

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

    // **A network with an active EVM lane needs an evm build, and this is where it finds out.**
    //
    // `build_block_template` cannot construct a valid template without the feature, and it says so
    // by PANICKING — at the first template, which is after boot, after IBD, and after an operator
    // has every reason to believe the node is fine. On testnet-11 and testnet-12 the lane is active
    // from DAA 0, so that panic is certain rather than conditional. Refusing here turns a late
    // crash into a startup message naming the fix.
    //
    // Read from the network's own params, not from a list of network names: a preset that gains or
    // loses the lane moves this with it, and the check cannot go stale.
    #[cfg(not(feature = "evm"))]
    {
        let params: kaspa_consensus_core::config::params::Params = args.network().into();
        if params.evm_activation_daa_score == 0 {
            return Err(ConfigError::EvmLaneRequiresEvmBuild(params.net.to_string()));
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
        if args.enable_compute {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--enable-compute"));
        }
        if args.evm_rpc_listen.is_some() {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--evm-rpc-listen"));
        }
        if args.unsafe_rpc {
            return Err(ConfigError::NodeProfileIncompatible(profile, "--unsaferpc"));
        }
    }
    if matches!(args.node_profile, NodeProfile::RecoverySync) && args.connect_peers.is_empty() {
        return Err(ConfigError::RecoverySyncRequiresConnect);
    }
    // The compute role rides the validator service — it signs with the validator's key and stakes
    // the validator's bond — so `--enable-compute` alone would silently do nothing.
    if args.enable_compute && !args.enable_validator {
        return Err(ConfigError::MissingDependentArg("--enable-compute", "--enable-validator"));
    }
    Ok(())
}

fn data_mount_free_percent(path: &Path) -> Option<f64> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) if parent != probe => probe = parent.to_path_buf(),
            _ => break,
        }
    }
    let probe = probe.canonicalize().unwrap_or(probe);

    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut best: Option<(usize, u64, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point();
        if probe.starts_with(mount) {
            let len = mount.as_os_str().len();
            if best.map(|(best_len, _, _)| len > best_len).unwrap_or(true) {
                best = Some((len, disk.available_space(), disk.total_space()));
            }
        }
    }
    best.and_then(|(_, available, total)| (total > 0).then(|| available as f64 / total as f64 * 100.0))
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

    // ADR-0042 PR-10: the PALW-RC identity (testnet-12) without its ruleset bundle.
    //
    // `Params::from(NetworkId)` returns the RC IDENTITY — the RC genesis, the frozen cadence, no
    // V1 PALW proof-of-work — because a `ConsensusV2` bundle is a function of genesis artifacts
    // no `From` impl holds. A node started here with no bundle is therefore on a DIFFERENT
    // ruleset from the real RC network: the bundle is inside `consensus_params_id`, so it will be
    // refused at the handshake rather than silently join a chain with no classes, no bonds and no
    // court.
    //
    // That refusal is correct and it is also opaque from the outside — "no peers" looks like a
    // networking problem. Say it here, once, in the words the operator needs, rather than letting
    // them infer it from a node that dials and gets dropped.
    if !matches!(params.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
        && params.genesis.hash == kaspa_consensus_core::config::genesis::PALW_RC_GENESIS.hash
    {
        warn!(
            "network {} is the PALW-RC identity but this node carries NO ConsensusV2 ruleset bundle.              Its consensus fingerprint therefore differs from a bundled node's and peers will refuse              the handshake — this node cannot join the RC network until the genesis artifacts are              installed. Nothing is wrong with the connection.",
            network
        );
    }

    // MISAKA Phase 4 (PALW LLM PoW, ADR-0021) startup rails. Header validation on a PALW-active
    // network replays a pinned-LLM inference per header; a node whose runtime is missing there
    // would price every honest header as failed PoW and follow nothing (ADR-0042 Decision 4 —
    // the panic is reserved for a runtime that registers and then breaks). Check the operator's
    // intent HERE, at startup, with actionable messages, before a peer is dialed.
    {
        // "Ever active on this network": is_active at the largest checkable score — false only
        // for `ForkActivation::never()`.
        let palw_ever_active = params.pow_palw_activation.is_active(u64::MAX - 1);
        // Phase 4b (algo_id = 5): the Ollama-runtime PALW network. Validation reaches a
        // host-local Ollama server, so check reachability and the model pin NOW.
        let palw_ollama_ever_active = params.pow_palw_ollama_activation.is_active(u64::MAX - 1);
        let devnet = network.network_type == kaspa_consensus_core::network::NetworkType::Devnet;
        // The fixture derives DIFFERENT tags than the pinned model — fixture rules are a
        // different network. `kaspa_pow::palw::fixture_permitted_on` confines them to devnet at the
        // point the tag is computed, so this rail no longer decides WHETHER the fixture applies; it
        // decides what to tell the operator about a variable that will not do what they think.
        //
        // Two cases, and they used to be one `exit(1)`:
        //
        // * PALW is active here and this is not devnet — the operator asked for fixture rules on a
        //   network that has real ones. Stop, with the message naming the variable: continuing
        //   would need a real worker they have not configured, and failing at the first relayed
        //   header is the outcome this whole block exists to prevent.
        // * PALW is NOT active here — the variable cannot affect a single tag on this network,
        //   because nothing on it computes one. Aborting was over-broad, and measurably so: it
        //   made one process unable to host a devnet consensus and a simnet consensus at once, so
        //   the integration suite could not be run in a single invocation whatever the variable was
        //   set to. Say it once and carry on.
        let fixture = kaspa_pow::palw::fixture_requested();
        if fixture && !devnet {
            if palw_ever_active || palw_ollama_ever_active {
                println!(
                    "MISAKA_PALW_POW_FIXTURE=1 is only honored on devnet: fixture PALW tags are a \
                     different rule set than the pinned model, and running them against {} would just \
                     fork you off the network at the first block. Unset it, or use --devnet.",
                    network
                );
                exit(1);
            }
            warn!(
                "MISAKA_PALW_POW_FIXTURE=1 is set but {} does not validate PALW proof-of-work — the \
                 variable is ignored here and changes nothing this node accepts.",
                network
            );
        }
        // Below, "fixture" must mean "the fixture is what this node validates", not "somebody
        // exported the variable" — on a non-devnet network the runtime checks still apply.
        let fixture = fixture && devnet;
        // ADR-0042 Decision 4 (PR-02): the consensus build carries no model runtime — kaspad is
        // the composition root that wires one in, and only where this network's rules can ever
        // demand an inference-priced tag. Everywhere else NOTHING is registered, so no
        // validation path can reach a model, or a model's failure modes: an algo-4/5 header on
        // such a network simply fails PoW.
        if (palw_ever_active || palw_ollama_ever_active) && !fixture {
            misaka_palw_pow_driver::install();
        }
        if palw_ollama_ever_active && !fixture {
            let model = match std::env::var(misaka_palw_pow_driver::PALW_OLLAMA_MODEL_ENV) {
                Ok(m) => m,
                Err(_) => {
                    println!(
                        "network {} validates PALW-Ollama (algo_id = 5) LLM proof-of-work.\nSet {}=<pinned model, e.g. \
                         qwen3.5:2b> (and optionally {}=http://127.0.0.1:11434), with `ollama serve` running and the \
                         model pulled — see docs/testnet10-palw-rollout-runbook.md.{}",
                        network,
                        misaka_palw_pow_driver::PALW_OLLAMA_MODEL_ENV,
                        misaka_palw_pow_driver::PALW_OLLAMA_URL_ENV,
                        if network.network_type == kaspa_consensus_core::network::NetworkType::Devnet {
                            "\nOr export MISAKA_PALW_POW_FIXTURE=1 for the model-free devnet fixture."
                        } else {
                            ""
                        }
                    );
                    exit(1);
                }
            };
            let url = std::env::var(misaka_palw_pow_driver::PALW_OLLAMA_URL_ENV)
                .unwrap_or_else(|_| misaka_palw_pow_driver::DEFAULT_OLLAMA_URL.to_string());
            let hostport = url.strip_prefix("http://").unwrap_or(&url).trim_end_matches('/').to_string();
            match std::net::TcpStream::connect_timeout(
                &hostport.parse().unwrap_or_else(|_| {
                    println!("{} must be http://host:port, got {url}", misaka_palw_pow_driver::PALW_OLLAMA_URL_ENV);
                    exit(1);
                }),
                Duration::from_secs(3),
            ) {
                Ok(_) => match misaka_palw_pow_driver::verify_ollama_model_pin(&url, &model) {
                    Ok(()) => info!("PALW-Ollama runtime: {url} serving the pinned model blob as {model}"),
                    Err(e) => {
                        println!("{e}");
                        exit(1);
                    }
                },
                Err(e) => {
                    println!(
                        "cannot reach the Ollama server at {url}: {e}\nStart it (`ollama serve`, or the systemd unit from \
                         scripts/misaka-palw-ollama-setup.sh) and pull the pinned model (`ollama pull {model}`)."
                    );
                    exit(1);
                }
            }
        }
        if palw_ever_active && !fixture {
            match std::env::var("PALW_WORKER") {
                Err(_) => {
                    println!(
                        "network {} validates PALW (algo_id = 4) LLM proof-of-work, which needs the \
                         pinned worker runtime.\nSet PALW_WORKER=<path to palw-worker> and \
                         MISAKA_PALW_GGUF=<path to {}>{}",
                        network,
                        kaspa_consensus_core::vlt::qwen35_pins::GGUF_FILENAME,
                        if network.network_type == kaspa_consensus_core::network::NetworkType::Devnet {
                            ", or export MISAKA_PALW_POW_FIXTURE=1 for the model-free devnet fixture."
                        } else {
                            "."
                        }
                    );
                    exit(1);
                }
                Ok(worker) => {
                    if !std::path::Path::new(&worker).is_file() {
                        println!("PALW_WORKER points at {worker}, which does not exist.");
                        exit(1);
                    }
                    if std::env::var("MISAKA_PALW_GGUF").is_err() {
                        println!(
                            "PALW_WORKER is set but MISAKA_PALW_GGUF is not — the worker refuses to run \
                             without the pinned {} (size+sha checked).",
                            kaspa_consensus_core::vlt::qwen35_pins::GGUF_FILENAME
                        );
                        exit(1);
                    }
                    // Class-pinned nets (ADR-0035): prove this runtime is in the network's
                    // determinism class BEFORE any peer is dialed — one probe inference now,
                    // with a clear message, instead of a silent self-fork at the first header.
                    // (The tag runner re-checks lazily via the same memoized probe, so a miner
                    // or harness that bypasses this rail is covered too.)
                    let net_id_bytes = network.to_string();
                    if kaspa_consensus_core::pow_layer0::palw_worker_calibration_v1(net_id_bytes.as_bytes()).is_some() {
                        info!("PALW class calibration: replaying the pinned probe (one inference — this takes seconds)…");
                        match kaspa_pow::palw::verify_worker_calibration(net_id_bytes.as_bytes()) {
                            Ok(()) => info!("PALW worker runtime verified in the pinned determinism class of {network}"),
                            Err(e) => {
                                println!("{e}");
                                exit(1);
                            }
                        }
                    }
                }
            }
        }
    }

    let config = Arc::new(
        ConfigBuilder::new(params).adjust_perf_params_to_consensus_params().apply_args(|config| args.apply_to_config(config)).build(),
    );

    let app_dir = get_app_dir_from_args(args);
    let db_dir = app_dir.join(network.to_prefixed()).join(DEFAULT_DATA_DIR);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), git::with_short_hash(version()));

    assert!(!db_dir.to_str().unwrap().is_empty());
    // The fingerprint this node will announce at the handshake, from ITS OWN side. Peers print
    // their own value in a rejection, so reading a fingerprint out of a received reject attributes
    // it to the wrong node — an error that cost a release cut on 2026-08-11. One line here makes
    // "is this binary the release?" answerable without a peer, which is what a flag day needs.
    info!("Consensus params fingerprint: {} (network {})", config.params.consensus_params_id(), config.params.net);

    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());
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
        // If we reached here the soft-upgrade ladder has run as far as it goes. It only climbs to
        // version 6; 7 (ADR-0020's EVM header fields) and 8 (ADR-0038's `palw_commitment`) changed
        // the bincode layout of the on-disk `Header`, and per ADR-0001 those are NOT migrated.
        //
        // Ask for a clean resync rather than asserting. The `assert_eq!` that stood here turned an
        // ordinary "your database is from a previous layout" into a panic — the exact failure mode
        // `LATEST_DB_VERSION` exists to prevent (mainnet-readiness audit §5). An operator upgrading
        // across a layout bump must be told to delete and resync, not shown a backtrace.
        let version = mcms.version().unwrap();
        if version != LATEST_DB_VERSION {
            info!(
                "Node database is version {version}; this build requires version {LATEST_DB_VERSION}, whose on-disk block-header layout is different and is not migrated."
            );
            is_db_reset_needed = request_database_deletion_approval(args.yes);
            continue 'db_upgrade;
        }
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
    // connect_peers means no DNS seeding and no outbound/inbound peers
    let outbound_target = if connect_peers.is_empty() { args.outbound_target } else { 0 };
    let inbound_limit = if connect_peers.is_empty() { args.inbound_limit } else { 0 };
    let dns_seeders = if connect_peers.is_empty() && !args.disable_dns_seeding { config.dns_seeders } else { &[] };

    // P2P bootstrap mode (design §9): make it explicit how peers are discovered and WHY
    // DNS seeding was or wasn't used — so an operator who passed --connect/--addpeer/
    // --nodnsseed understands why the seed path was skipped.
    if !connect_peers.is_empty() {
        info!("P2P bootstrap: explicit --connect peers only ({}) — DNS seed and peer discovery disabled", connect_peers.len());
    } else if !dns_seeders.is_empty() {
        info!(
            "P2P bootstrap: DNS seed ({} seeder(s)){}",
            dns_seeders.len(),
            if args.add_peers.is_empty() { "" } else { " + explicit --addpeer peers" }
        );
    } else if args.disable_dns_seeding {
        info!("P2P bootstrap: DNS seed disabled (--nodnsseed) — using explicit peers + the address manager");
    } else if args.add_peers.is_empty() {
        // The dead case, and it deserves a WARN rather than the info line it used to share with
        // the benign one: no seeders, no --connect, no --addpeer. On a network that has run
        // before, the address manager still holds peers and this is merely noisy; on a NEW
        // network — which is exactly when `dns_seeders` is empty — the address book is empty too,
        // and the node will sit alone forever without ever saying why. A new network's discovery
        // names are an operational step (someone must own and delegate them), so the software
        // cannot fix this for the operator; it can refuse to be quiet about it.
        warn!(
            "P2P bootstrap: this network has NO DNS seeders and no --connect/--addpeer peers were given. \
             If the address manager is empty (a fresh datadir on a new network) this node will not find \
             anyone. Pass --addpeer=<host:port> with a peer you know, or configure seeders for the network."
        );
    } else {
        info!("P2P bootstrap: no DNS seeders configured for this network — using the explicit --addpeer peers + the address manager");
    }

    let grpc_server_addr = args.rpclisten.unwrap_or(ContextualNetAddress::loopback()).normalize(config.default_rpc_port());

    let core = Arc::new(Core::new());

    // ---

    let tick_service = Arc::new(TickService::new());
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

    let (address_manager, port_mapping_extender_svc) = AddressManager::new(config.clone(), meta_db.clone(), tick_service.clone());

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
    // One gate, consulted by mining, both validator paths, and compute. Scoped to the networks that
    // have peers to be wrong about, mirroring `has_sufficient_peer_connectivity`: a peerless
    // devnet/simnet node has no competing branch to overlook, so holding it back only stalls tests.
    // Restored from the meta DB before anyone can consult it: a quarantine that a restart clears is
    // not a quarantine, and this is the one object every signer asks for permission.
    let chain_participation_store = Arc::new(ChainParticipationStore::new(meta_db.clone()));
    let chain_participation = Arc::new(
        ChainParticipationGate::new(
            matches!(
                config.net.network_type,
                kaspa_consensus_core::network::NetworkType::Mainnet | kaspa_consensus_core::network::NetworkType::Testnet
            ) || args.enforce_chain_participation,
        )
        .with_persistence(chain_participation_store),
    );
    // ADR-0025's operator intervention. Placed after restore and before anything consults the
    // gate, so the clear is indistinguishable from having been resolved before boot. WARN on
    // every firing on purpose: the flag re-clears each boot it is left in place for, and a unit
    // file that silently neuters quarantine forever is the exact hazard the log line names.
    if args.clear_quarantine {
        if chain_participation.operator_clear_quarantine() {
            warn!(
                "--clear-quarantine: a persisted Quarantined participation state was CLEARED by operator override. This \
                 node resumes normal participation on the chain it is on. Remove the flag from the service unit — left in \
                 place it re-clears on every restart, and a quarantine that a flag always clears is not a quarantine."
            );
        } else {
            info!("--clear-quarantine: nothing to clear (participation is not quarantined).");
        }
    }
    match chain_participation.state() {
        kaspa_core::chain_participation::ChainParticipation::Ready => {}
        state => warn!(
            "Chain participation restored as {} from the previous run: this node will NOT mine, attest, or report itself \
             synced until that resolves. It stopped because an IBD left it on a chain it could not vouch for.",
            state.as_str()
        ),
    }
    let mining_rule_engine = Arc::new(MiningRuleEngine::new(
        consensus_manager.clone(),
        config.clone(),
        processing_counters.clone(),
        tick_service.clone(),
        hub.clone(),
        mining_rules,
        chain_participation.clone(),
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

    // MISAKA PALW v2 (Land stage): monitor a palw-agent when one is configured. Observation
    // and a capability handle only — an absent or quarantined agent withdraws v2 compute
    // capability (which nothing consensus-visible consumes yet) and the node continues
    // validator-only, exactly as the VPS design's failure policy requires. Independent of
    // `--enable-validator`: watching a runtime is not a validator role.
    let _palw_agent_capability = args.compute_endpoint.as_ref().map(|endpoint| {
        // Accept both the bare socket path and the design docs' `unix://` URI spelling.
        let path = endpoint.strip_prefix("unix://").unwrap_or(endpoint);
        crate::palw_agent::spawn_palw_agent_monitor(PathBuf::from(path))
    });

    // kaspa-pq Phase 11 (ADR-0010): in-process DNS-overlay validator service. Built only
    // when `--enable-validator` is set (so default node behavior is unchanged) and after
    // `flow_context`, which it uses to submit attestation-shard transactions.
    let validator_service = if args.enable_validator {
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
        // MISAKA Verified LLM Token-Weighted BFT: the compute role rides the validator service, so
        // it is only reachable behind `--enable-validator`. `ComputeRole::new` decides whether it
        // actually starts — the runtime has to probe as the consensus-registered profile, which on
        // a network with an empty model cost table (every shipped preset) it cannot.
        let compute = ComputeConfig {
            enabled: args.enable_compute,
            worker_bin: args.compute_worker.as_ref().map(PathBuf::from),
            work_dir: args.compute_work_dir.as_ref().map(PathBuf::from).unwrap_or_else(std::env::temp_dir),
            timeout: Duration::from_secs(args.compute_timeout_secs.unwrap_or(DEFAULT_COMPUTE_TIMEOUT_SECS)),
            prompt_path: args.compute_prompt.as_ref().map(PathBuf::from),
            max_tokens: args.compute_max_tokens.unwrap_or(DEFAULT_COMPUTE_MAX_TOKENS),
            auto_challenge: args.compute_auto_challenge,
            fixture_job_limit: args.compute_fixture_job_limit,
            // Beside the app dir, not the compute work dir: the work dir defaults to the system
            // temp directory, and a quota that evaporates on reboot is not a quota.
            fixture_state_path: Some(app_dir.join("fixture-quota.json")),
        };
        let validator_config = ValidatorConfig {
            mode,
            key_path: args.validator_key.clone(),
            stake_bond: args.stake_bond.clone(),
            state_path: Some(state_path),
            address_prefix: config.prefix(),
            // ADR-0009 Addendum A.3: the network discriminator is the per-network genesis hash,
            // the same value consensus binds into every overlay message it verifies.
            network_id: config.params.genesis.hash.as_byte_slice().to_vec(),
            vlt: config.params.dns_params.as_ref().map(|p| p.vlt),
            compute,
            tkn_fixture_transfers: args.tkn_fixture_transfers.clone(),
            tkn_fixture_burns: args.tkn_fixture_burns.clone(),
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
        )))
    } else {
        None
    };

    let p2p_service = Arc::new(P2pService::new(
        flow_context.clone(),
        connect_peers,
        add_peers,
        p2p_server_addr,
        outbound_target,
        inbound_limit,
        dns_seeders,
        config.default_p2p_port(),
        p2p_tower_counters.clone(),
    ));

    // kaspa-pq Phase 11 (ADR-0010): expose the in-process validator service's status via
    // the `getValidatorStatus` RPC (None when `--enable-validator` is off).
    // ADR-0042: the in-process PALW-RC producer. Everything it needs that is not chain state comes
    // from the operator — a key it did not generate, the bond that key signs for, and where the
    // reward goes. A missing one is a startup refusal rather than a producer that runs and cannot
    // publish; a hash-only network is a refusal for a different reason, and both say which.
    let palw_producer_service = if args.palw_produce {
        let base_class_id = match &config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.base_class_id),
            _ => None,
        };
        match (base_class_id, &args.palw_producer_key, &args.palw_producer_bond, &args.palw_producer_pay_address) {
            (Some(class_id), Some(key_path), Some(bond), Some(pay_address)) => {
                Some(Arc::new(crate::palw_producer::PalwProducerService::new(
                    crate::palw_producer::PalwProducerConfig {
                        key_path: key_path.clone(),
                        bond: bond.clone(),
                        pay_address: pay_address.clone(),
                        address_prefix: config.prefix(),
                        network_id: config.params.net.to_string(),
                        // Beside the per-network data dir: the material behind a published attempt
                        // is a data-availability obligation for `trace_retention_daa`, so it has to
                        // outlive a restart the way the chain it answers to does.
                        retention_dir: app_dir.join(network.to_prefixed()).join("palw-retention"),
                        class_id,
                    },
                    consensus_manager.clone(),
                    mining_manager.clone(),
                    flow_context.clone(),
                )))
            }
            (None, _, _, _) => {
                warn!(
                    "--palw-produce was given but {} declares no ConsensusV2 ruleset — nothing to produce, producer not started",
                    config.params.net
                );
                None
            }
            _ => {
                warn!(
                    "--palw-produce needs --palw-producer-key, --palw-producer-bond and --palw-producer-pay-address — producer not started"
                );
                None
            }
        }
    } else {
        None
    };

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
    // Kept for the PALW panel service below — `rpc_core_service` consumes the originals.
    let flow_context_for_palw_panel = flow_context.clone();
    let config_for_palw_panel = config.clone();
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
    async_runtime.register(tick_service);
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
    // ADR-0042: the PALW-RC block producer. Registered only when it was asked for AND the network
    // actually has a ConsensusV2 lane — a producer on a hash-only chain would build templates for
    // an algo nobody declares and log a refusal every few hundred milliseconds.
    // ADR-0042 Decision 7: the panel service — a bonded node's seat duties, and (funded) the
    // submitter that carries the assembled quorum to the chain. Deliberately independent of
    // --palw-produce: a validator that never mines still judges.
    let palw_panel_service = if args.palw_panel {
        let v2 = matches!(
            config_for_palw_panel.params.palw_consensus_mode,
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)
        );
        match (v2, &args.palw_producer_key, &args.palw_producer_bond) {
            (true, Some(key_path), Some(bond)) => Some(Arc::new(crate::palw_panel::PalwPanelService::new(
                crate::palw_panel::PalwPanelConfig {
                    key_path: key_path.clone(),
                    bond: bond.clone(),
                    fee_outpoint: args.palw_fee_outpoint.clone(),
                    state_dir: app_dir.join(network.to_prefixed()).join("palw-panel"),
                },
                consensus_manager.clone(),
                flow_context_for_palw_panel.clone(),
                config_for_palw_panel.clone(),
            ))),
            (false, _, _) => {
                warn!(
                    "--palw-panel was given but {} declares no ConsensusV2 ruleset — no panels exist here, service not started",
                    config_for_palw_panel.params.net
                );
                None
            }
            _ => {
                warn!("--palw-panel needs --palw-producer-key and --palw-producer-bond (the seat identity) — service not started");
                None
            }
        }
    } else {
        None
    };

    if let Some(palw_panel_service) = palw_panel_service {
        async_runtime.register(palw_panel_service);
    }
    if let Some(palw_producer_service) = palw_producer_service {
        async_runtime.register(palw_producer_service)
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

    /// The compute role signs with the validator's key and stakes the validator's bond, so
    /// `--enable-compute` on its own has nothing to hang off. Failing at startup beats a node that
    /// looks configured for compute and silently never does any.
    #[test]
    fn enable_compute_requires_the_validator_service() {
        let args = parse(&["--enable-compute"]);
        assert!(matches!(validate_args(&args), Err(ConfigError::MissingDependentArg("--enable-compute", "--enable-validator"))));
        assert!(validate_args(&parse(&["--enable-compute", "--enable-validator"])).is_ok());
        assert!(validate_args(&parse(&[])).is_ok());

        let pruned = parse(&["--node-profile=bootstrap-pruned", "--enable-compute", "--enable-validator"]);
        assert!(matches!(validate_args(&pruned), Err(ConfigError::NodeProfileIncompatible(_, _))));
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
}
