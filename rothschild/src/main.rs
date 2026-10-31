// ML-DSA transport port provenance: 67bc4d4ca34247165712cdc99b5a72f25f314539.
// Measurement mode is an explicit local-devnet tool, never a protocol change.
#[cfg(unix)]
mod measurement;
use std::{collections::HashMap, io::Read as _, str::FromStr, sync::Arc, time::Duration};

use clap::{Arg, ArgAction, Command};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{
    config::params::Params,
    constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION},
    hashing::{
        sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
        sighash_type::SIG_HASH_ALL,
    },
    mass::MassCalculator,
    network::{NetworkId, NetworkType},
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        MutableTransaction, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint,
        TransactionOutput, UtxoEntry,
    },
};
use kaspa_core::{info, kaspad_env::version, time::unix_now, warn};
use kaspa_grpc_client::{ClientPool, GrpcClient};
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_pq_validator_core::{VALIDATOR_SEED_LEN, ValidatorKey, load_validator_seed, relay_fee_for_compute_mass};
use kaspa_rpc_core::{RpcUtxoEntry, api::rpc::RpcApi, notify::mode::NotificationMode};
use kaspa_txscript::{MLDSA87_SIG_LEN, MLDSA87_TX_CONTEXT, pay_to_address_script, script_builder::ScriptBuilder};
use parking_lot::Mutex;
use rand::{Rng, RngCore};
use rayon::prelude::*;
use tokio::time::{Instant, MissedTickBehavior, interval};

const DEFAULT_SEND_AMOUNT: u64 = 10 * SOMPI_PER_KASPA;
const MILLIS_PER_TICK: u64 = 10;
/// The largest payload a payload-carrying transaction may carry (the CLI bound).
const MAX_PAYLOAD_SIZE: usize = 20_000;

struct Stats {
    num_txs: usize,
    num_utxos: usize,
    utxos_amount: u64,
    num_outs: usize,
    since: u64,
}

pub struct Args {
    pub measurement_capabilities: bool,
    pub initial_utxos: Option<usize>,
    pub measurement_duration_secs: u64,
    pub measurement_funding_timeout_secs: u64,
    pub measurement_log: Option<String>,
    pub private_key_file: Option<String>,
    pub private_key_stdin: bool,
    pub tps: u64,
    pub rpc_server: String,
    pub threads: u8,
    pub unleashed: bool,
    pub addr: Option<String>,
    pub priority_fee: u64,
    pub randomize_fee: bool,
    pub payload_size: usize,
    pub network: NetworkId,
}

impl Args {
    fn parse() -> Self {
        let m = cli().get_matches();
        let network = m.get_one::<String>("network").cloned().unwrap();
        let network_type = NetworkType::from_str(&network).expect("Invalid network type");
        // `testnet` means testnet-12 here: the only testnet this tool drives (ML-DSA-87 outputs, ADR-0019).
        let network_id = match network_type {
            NetworkType::Testnet => NetworkId::with_suffix(NetworkType::Testnet, 12),
            other => NetworkId::new(other),
        };
        let default_rpc_server = format!("localhost:{}", network_type.default_rpc_port());

        Args {
            measurement_capabilities: m.get_flag("measurement-capabilities"),
            initial_utxos: m.get_one::<u32>("initial-utxos").map(|v| *v as usize),
            measurement_duration_secs: *m.get_one::<u64>("measurement-duration-secs").unwrap(),
            measurement_funding_timeout_secs: *m.get_one::<u64>("measurement-funding-timeout-secs").unwrap(),
            measurement_log: m.get_one::<String>("measurement-log").cloned(),
            private_key_file: m.get_one::<String>("private-key-file").cloned(),
            private_key_stdin: m.get_one::<bool>("private-key-stdin").cloned().unwrap_or(false),
            tps: m.get_one::<u64>("tps").cloned().unwrap(),
            network: network_id,
            rpc_server: m.get_one::<String>("rpcserver").cloned().unwrap_or(default_rpc_server),
            threads: m.get_one::<u8>("threads").cloned().unwrap(),
            unleashed: m.get_one::<bool>("unleashed").cloned().unwrap_or(false),
            addr: m.get_one::<String>("addr").cloned(),
            priority_fee: m.get_one::<u64>("priority-fee").cloned().unwrap_or(0),
            randomize_fee: m.get_one::<bool>("randomize-fee").cloned().unwrap_or(false),
            payload_size: m.get_one::<usize>("payload-size").cloned().unwrap_or(0),
        }
    }
}

pub fn cli() -> Command {
    Command::new("rothschild")
        .about(format!("{} (rothschild) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("measurement-capabilities")
                .long("measurement-capabilities")
                .action(ArgAction::SetTrue)
                .help("Print truthful measurement support before key loading or RPC connection, then exit"),
        )
        .arg(
            Arg::new("initial-utxos")
                .long("initial-utxos")
                .value_parser(clap::value_parser!(u32).range(1..=10_000))
                .requires("measurement-log")
                .help("Local-devnet only: prepare and confirm this many independent funding outputs before load"),
        )
        .arg(
            Arg::new("measurement-log")
                .long("measurement-log")
                .value_name("fresh-path")
                .requires("initial-utxos")
                .help("Exclusive new mode-0600 JSONL trace; existing paths are refused"),
        )
        .arg(
            Arg::new("measurement-duration-secs")
                .long("measurement-duration-secs")
                .default_value("600")
                .value_parser(clap::value_parser!(u64).range(1..=3600)),
        )
        .arg(
            Arg::new("measurement-funding-timeout-secs")
                .long("measurement-funding-timeout-secs")
                .default_value("600")
                .value_parser(clap::value_parser!(u64).range(1..=3600)),
        )
        .arg(
            Arg::new("private-key-file")
                .long("private-key-file")
                .value_name("path")
                .help("ML-DSA-87 seed (64 hex chars) read from a file, mode 0600 (preferred)"),
        )
        .arg(
            Arg::new("private-key-stdin")
                .long("private-key-stdin")
                .action(ArgAction::SetTrue)
                .help("ML-DSA-87 seed (64 hex chars) read from stdin"),
        )
        .arg(
            Arg::new("tps")
                .long("tps")
                .short('t')
                .value_name("tps")
                .default_value("1")
                .value_parser(clap::value_parser!(u64))
                .help("Transactions per second"),
        )
        .arg(
            Arg::new("network")
                .long("network")
                .short('n')
                .value_name("network")
                .default_value("testnet")
                .value_parser(["testnet", "devnet"])
                .help("Network to use: testnet means testnet-12, devnet means devnet"),
        )
        .arg(
            Arg::new("rpcserver")
                .long("rpcserver")
                .short('s')
                .value_name("rpcserver")
                .help("RPC server (defaults: testnet=16210, devnet=16610"),
        )
        .arg(
            Arg::new("threads")
                .long("threads")
                .default_value("2")
                .value_parser(clap::value_parser!(u8))
                .help("The number of threads to use for TX generation. Set to 0 to use 1 thread per core. Default is 2."),
        )
        .arg(Arg::new("unleashed").long("unleashed").action(ArgAction::SetTrue).hide(true).help("Allow higher TPS"))
        .arg(
            Arg::new("addr").long("to-addr").short('a').value_name("addr").help("PubKeyHashMlDsa87 address to send to (default: own)"),
        )
        .arg(
            Arg::new("priority-fee")
                .long("priority-fee")
                .short('f')
                .value_name("priority-fee")
                .default_value("0")
                .value_parser(clap::value_parser!(u64))
                .help("Transaction priority fee"),
        )
        .arg(
            Arg::new("randomize-fee")
                .long("randomize-fee")
                .short('r')
                .value_name("randomize-fee")
                .action(ArgAction::SetTrue)
                .default_value("false")
                .help("Randomize transaction priority fee"),
        )
        .arg(
            Arg::new("payload-size")
                .long("payload-size")
                .short('p')
                .value_name("payload-size")
                .default_value("0")
                .value_parser(clap::value_parser!(usize))
                .help("Random payload size in bytes (max 20000)"),
        )
}

async fn new_rpc_client(subscription_context: &SubscriptionContext, address: &str) -> GrpcClient {
    GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", address),
        Some(subscription_context.clone()),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .unwrap()
}

struct ClientPoolArg {
    tx: Transaction,
    stats: Arc<Mutex<Stats>>,
    selected_utxos_len: usize,
    selected_utxos_amount: u64,
    pending_len: usize,
    utxos_len: usize,
}

struct TxConfig {
    priority_fee: u64,
    randomize_fee: bool,
    payload_size: usize,
}

/// Everything a transaction of this run shares: the signer, the chain's mass parameters, the
/// recipient script and the output count. Fees are derived from it, not from a flat rate.
struct TxPlan<'a> {
    key: &'a ValidatorKey,
    params: &'a Params,
    to_spk: ScriptPublicKey,
    num_outs: u64,
}

fn mass_calc(p: &Params) -> MassCalculator {
    MassCalculator::new(p.mass_per_tx_byte, p.mass_per_script_pub_key_byte, p.mass_per_sig_op, p.storage_mass_parameter)
}

/// Read the ML-DSA-87 seed from a file or from stdin only. The seed is never echoed.
fn load_seed(args: &Args) -> [u8; VALIDATOR_SEED_LEN] {
    if let Some(path) = &args.private_key_file {
        match load_validator_seed(path) {
            Ok(seed) => seed,
            Err(e) => {
                eprintln!("refusing to start: {e}");
                std::process::exit(1);
            }
        }
    } else if args.private_key_stdin {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).expect("failed to read the private key from stdin");
        let mut seed = [0u8; VALIDATOR_SEED_LEN];
        if faster_hex::hex_decode(s.trim().as_bytes(), &mut seed).is_err() {
            eprintln!("refusing to start: the seed on stdin must be {} hex characters (not echoed)", VALIDATOR_SEED_LEN * 2);
            std::process::exit(1);
        }
        seed
    } else {
        eprintln!("refusing to start: pass --private-key-file <path> or --private-key-stdin (an ML-DSA-87 seed)");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    kaspa_core::log::init_logger(None, "");
    let args = Args::parse();
    if args.measurement_capabilities {
        println!(
            r#"{{"key_scheme":"MLDSA87","devnet_fanout":{},"trace_version":{},"rpc_success_txid_log":true}}"#,
            cfg!(unix),
            if cfg!(unix) { 1 } else { 0 }
        );
        return;
    }

    if args.initial_utxos.is_some() {
        #[cfg(unix)]
        if let Err(error) = measurement::validate_args(&args) {
            eprintln!("measurement configuration refused: {error}");
            std::process::exit(1);
        }
        #[cfg(not(unix))]
        {
            eprintln!("measurement requires Unix trace permissions");
            std::process::exit(1);
        }
    }
    let address_prefix = Prefix::from(args.network);
    let params = Params::from(args.network);

    let stats = Arc::new(Mutex::new(Stats { num_txs: 0, since: unix_now(), num_utxos: 0, utxos_amount: 0, num_outs: 0 }));
    let subscription_context = SubscriptionContext::new();
    let rpc_client = GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{}", args.rpc_server),
        Some(subscription_context.clone()),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .expect("Critical error: failed to connect to the RPC server.");

    info!("Connected to RPC");

    let mut pending: HashMap<TransactionOutpoint, Instant> = HashMap::new();

    // Never log the seed: the funding address is enough to operate.
    let seed = load_seed(&args);
    let key = ValidatorKey::from_seed(seed);
    let kaspa_addr = key.funding_address(address_prefix);

    let kaspa_to_addr = match args.addr.as_ref() {
        None => kaspa_addr.clone(),
        Some(addr_str) => {
            let to = Address::try_from(addr_str.clone()).expect("bad --to-addr");
            if to.version != Version::PubKeyHashMlDsa87 {
                eprintln!("refusing to start: --to-addr must be a PubKeyHashMlDsa87 address (t12 outputs), got another version");
                std::process::exit(1);
            }
            to
        }
    };

    assert!(args.payload_size <= MAX_PAYLOAD_SIZE, "payload-size can be max {MAX_PAYLOAD_SIZE}");

    let tx_config = TxConfig { priority_fee: args.priority_fee, randomize_fee: args.randomize_fee, payload_size: args.payload_size };

    rayon::ThreadPoolBuilder::new().num_threads(args.threads as usize).build_global().unwrap();

    let mut log_message = format!(
        "Using Rothschild with:\n\
        \tnetwork: {}\n\
        \tfrom address: {}\n\
        \tsignature: ML-DSA-87 (PubKeyHashMlDsa87)",
        args.network,
        String::from(&kaspa_addr)
    );
    if args.addr.is_some() {
        log_message.push_str(&format!("\n\tto address: {}", String::from(&kaspa_to_addr)));
    }
    if args.priority_fee != 0 {
        log_message.push_str(&format!(
            "\n\tpriority fee: {} SOMPS {}",
            tx_config.priority_fee,
            if tx_config.randomize_fee { "[randomize]" } else { "" }
        ));
    }
    if args.payload_size != 0 {
        log_message.push_str(&format!("\n\tpayload size: {} random bytes", tx_config.payload_size,));
    }
    info!("{}", log_message);

    let info = rpc_client.get_block_dag_info().await.expect("Failed to get block dag info.");

    assert_eq!(info.network, args.network, "connected node differs from explicitly selected network");
    let coinbase_maturity = match info.network.suffix {
        Some(11) => panic!("TN11 is not supported on this version"),
        None | Some(_) => params.coinbase_maturity(),
    };
    info!(
        "Node block-DAG info: \n\tNetwork: {}, \n\tBlock count: {}, \n\tHeader count: {}, \n\tDifficulty: {},
\tMedian time: {}, \n\tDAA score: {}, \n\tPruning point: {}, \n\tTips: {}, \n\t{} virtual parents: ...{}, \n\tCoinbase maturity: {}",
        info.network,
        info.block_count,
        info.header_count,
        info.difficulty,
        info.past_median_time,
        info.virtual_daa_score,
        info.pruning_point_hash,
        info.tip_hashes.len(),
        info.virtual_parent_hashes.len(),
        info.virtual_parent_hashes.last().unwrap(),
        coinbase_maturity,
    );

    if args.initial_utxos.is_some() {
        #[cfg(unix)]
        {
            if let Err(error) =
                measurement::run(&args, &key, &params, &rpc_client, &kaspa_addr, params.coinbase_spend_maturity()).await
            {
                eprintln!("measurement stopped: {error}");
                std::process::exit(1);
            }
            return;
        }
        #[cfg(not(unix))]
        {
            eprintln!("measurement requires Unix exclusive mode-0600 trace support");
            std::process::exit(1);
        }
    }

    const CLIENT_POOL_SIZE: usize = 8;
    let mut rpc_clients = Vec::with_capacity(CLIENT_POOL_SIZE);
    for _ in 0..CLIENT_POOL_SIZE {
        rpc_clients.push(Arc::new(new_rpc_client(&subscription_context, &args.rpc_server).await));
    }

    let submit_tx_pool = ClientPool::new(rpc_clients, 1000);
    let _ = submit_tx_pool.start(|c, arg: ClientPoolArg| async move {
        let ClientPoolArg { tx, stats, selected_utxos_len, selected_utxos_amount, pending_len, utxos_len } = arg;
        match c.submit_transaction(tx.as_ref().into(), false).await {
            Ok(accepted_txid) => {
                // RPC admission only, not mined/accepted-chain confirmation.
                info!("MEASURE_SUBMIT rpc_success=true txid={} local_txid={} payload_bytes={} committed_storage_mass={} unix_ms={}",
                    accepted_txid,tx.id(),tx.payload.len(),tx.mass(),unix_now());
                if accepted_txid != tx.id() {
                    warn!("RPC returned transaction id different from submitted transaction: {} != {}",accepted_txid,tx.id());
                }
                let mut stats = stats.lock();
                stats.num_txs += 1;
                stats.num_utxos += selected_utxos_len;
                stats.utxos_amount += selected_utxos_amount;
                stats.num_outs += tx.outputs.len();
                let now = unix_now();
                let time_past = now - stats.since;
                if time_past > 10_000 {
                    info!(
                        "Tx rate: {:.1}/sec, avg UTXO amount: {}, avg UTXOs per tx: {}, avg outs per tx: {}, estimated available UTXOs: {}",
                        1000f64 * (stats.num_txs as f64) / (time_past as f64),
                        stats.utxos_amount / stats.num_utxos as u64,
                        stats.num_utxos / stats.num_txs,
                        stats.num_outs / stats.num_txs,
                        utxos_len.saturating_sub(pending_len),
                    );
                    stats.since = now;
                    stats.num_txs = 0;
                    stats.num_utxos = 0;
                    stats.utxos_amount = 0;
                    stats.num_outs = 0;
                }
            }
            Err(e) => {
                let mut tx = tx;
                tx.finalize();
                warn!("RPC error when submitting {}: {}", tx.id(), e);
            }
        }
        false
    });
    let tx_sender = submit_tx_pool.sender();

    let target_tps = args.tps.min(if args.unleashed { u64::MAX } else { 100 });
    let should_tick_per_second = target_tps * MILLIS_PER_TICK / 1000 == 0;
    let avg_txs_per_tick = if should_tick_per_second { target_tps } else { target_tps * MILLIS_PER_TICK / 1000 };
    let mut utxos = refresh_utxos(&rpc_client, kaspa_addr.clone(), &mut pending, coinbase_maturity).await;
    let mut ticker = interval(Duration::from_millis(if should_tick_per_second { 1000 } else { MILLIS_PER_TICK }));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let plan = TxPlan { key: &key, params: &params, to_spk: pay_to_address_script(&kaspa_to_addr), num_outs: 2 };

    let mut maximize_inputs = false;
    let mut last_refresh = unix_now();
    // This allows us to keep track of the UTXOs we already tried to use for this period
    // until the UTXOs are refreshed. At that point, this will be reset as well.
    let mut next_available_utxo_index = 0;
    // Tracker so we can try to send as close as possible to the target TPS
    let mut remaining_txs_in_interval = target_tps;

    loop {
        ticker.tick().await;
        maximize_inputs = should_maximize_inputs(maximize_inputs, &utxos, &pending);
        let txs_to_send = if remaining_txs_in_interval > avg_txs_per_tick * 2 {
            remaining_txs_in_interval -= avg_txs_per_tick;
            avg_txs_per_tick
        } else {
            let count = remaining_txs_in_interval;
            remaining_txs_in_interval = target_tps;
            count
        };

        let now = unix_now();
        let has_funds = maybe_send_tx(
            txs_to_send,
            &tx_sender,
            &plan,
            &mut utxos,
            &mut pending,
            stats.clone(),
            maximize_inputs,
            &mut next_available_utxo_index,
            &tx_config,
        )
        .await;
        if !has_funds {
            info!("Has not enough funds");
        }
        if !has_funds || now - last_refresh > 60_000 {
            info!("Refetching UTXO set");
            tokio::time::sleep(Duration::from_millis(100)).await; // We don't want this operation to be too frequent since its heavy on the node, so we wait some time before executing it.
            utxos = refresh_utxos(&rpc_client, kaspa_addr.clone(), &mut pending, coinbase_maturity).await;
            last_refresh = unix_now();
            next_available_utxo_index = 0;
            pause_if_mempool_is_full(&rpc_client).await;
        }
        clean_old_pending_outpoints(&mut pending);
    }
}

fn should_maximize_inputs(
    old_value: bool,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    pending: &HashMap<TransactionOutpoint, Instant>,
) -> bool {
    let estimated_utxos = if utxos.len() > pending.len() { utxos.len() - pending.len() } else { 0 };
    if !old_value && estimated_utxos > 1_000_000 {
        info!("Starting to maximize inputs");
        true
    } else if old_value && estimated_utxos < 500_000 {
        info!("Stopping to maximize inputs");
        false
    } else {
        old_value
    }
}

async fn pause_if_mempool_is_full(rpc_client: &GrpcClient) {
    loop {
        let mempool_size = rpc_client.get_info().await.unwrap().mempool_size;
        if mempool_size < 200_000 {
            break;
        }

        const PAUSE_DURATION: u64 = 10;
        info!("Mempool has {} entries. Pausing for {} seconds to reduce mempool pressure", mempool_size, PAUSE_DURATION);
        tokio::time::sleep(Duration::from_secs(PAUSE_DURATION)).await;
    }
}

async fn refresh_utxos(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    pending: &mut HashMap<TransactionOutpoint, Instant>,
    coinbase_maturity: u64,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    populate_pending_outpoints_from_mempool(rpc_client, kaspa_addr.clone(), pending).await;
    fetch_spendable_utxos(rpc_client, kaspa_addr, coinbase_maturity, pending).await
}

async fn populate_pending_outpoints_from_mempool(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    pending_outpoints: &mut HashMap<TransactionOutpoint, Instant>,
) {
    let entries = rpc_client.get_mempool_entries_by_addresses(vec![kaspa_addr], true, false).await.unwrap();
    let now = Instant::now();

    for entry in entries {
        for entry in entry.sending {
            for input in entry.transaction.inputs {
                pending_outpoints.insert(input.previous_outpoint.into(), now);
            }
        }
    }
}

async fn fetch_spendable_utxos(
    rpc_client: &GrpcClient,
    kaspa_addr: Address,
    coinbase_maturity: u64,
    pending: &mut HashMap<TransactionOutpoint, Instant>,
) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let resp = rpc_client.get_utxos_by_addresses(vec![kaspa_addr]).await.unwrap();
    let dag_info = rpc_client.get_block_dag_info().await.unwrap();

    let mut utxos = resp
        .into_iter()
        .filter(|entry| {
            is_utxo_spendable(
                &entry.utxo_entry,
                dag_info.virtual_daa_score,
                coinbase_maturity,
            )
        })
        .map(|entry| {
            (
                TransactionOutpoint::from(entry.outpoint),
                UtxoEntry::from(entry.utxo_entry),
            )
        })
        // Eliminates UTXOs we already tried to spend so we don't try to spend them again in this period
        .filter(|(outpoint, _)| !pending.contains_key(outpoint))
        .collect::<Vec<_>>();
    utxos.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    utxos
}

fn is_utxo_spendable(entry: &RpcUtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    let needed_confs = if !entry.is_coinbase {
        10
    } else {
        coinbase_maturity * 2 // TODO: We should compare with sink blue score in the case of coinbase
    };
    entry.block_daa_score + needed_confs < virtual_daa_score
}

#[allow(clippy::too_many_arguments)]
async fn maybe_send_tx(
    txs_to_send: u64,
    tx_sender: &async_channel::Sender<ClientPoolArg>,
    plan: &TxPlan<'_>,
    utxos: &mut [(TransactionOutpoint, UtxoEntry)],
    pending: &mut HashMap<TransactionOutpoint, Instant>,
    stats: Arc<Mutex<Stats>>,
    maximize_inputs: bool,
    next_available_utxo_index: &mut usize,
    tx_config: &TxConfig,
) -> bool {
    let num_outs = if maximize_inputs { 1 } else { plan.num_outs };
    let plan = TxPlan { key: plan.key, params: plan.params, to_spk: plan.to_spk.clone(), num_outs };

    let mut has_fund = false;

    let selected_utxos_groups = (0..txs_to_send)
        .map(|_| {
            let (selected_utxos, selected_amount) =
                select_utxos(utxos, DEFAULT_SEND_AMOUNT, &plan, maximize_inputs, next_available_utxo_index, tx_config);
            if selected_amount == 0 {
                return None;
            }

            // If any iteration successfully selected UTXOs, we assume to still
            // have funds in this tick
            has_fund = true;

            let now = Instant::now();
            for input in selected_utxos.iter() {
                pending.insert(input.0, now);
            }

            Some((selected_utxos, selected_amount))
        })
        .collect::<Vec<_>>();

    if !has_fund {
        return false;
    }

    let txs = selected_utxos_groups
        .into_par_iter()
        .map(|utxo_option| {
            let (selected_utxos, selected_amount) = utxo_option?;
            match generate_tx(&plan, &selected_utxos, selected_amount, tx_config.payload_size) {
                Ok(tx) => Some((tx, selected_utxos.len(), selected_utxos.into_iter().map(|(_, entry)| entry.amount).sum::<u64>())),
                Err(e) => {
                    warn!("could not build a transaction: {e}");
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    for (tx, selected_utxos_len, selected_utxos_amount) in txs.into_iter().flatten() {
        tx_sender
            .send(ClientPoolArg {
                tx,
                stats: stats.clone(),
                selected_utxos_len,
                selected_utxos_amount,
                pending_len: pending.len(),
                utxos_len: utxos.len(),
            })
            .await
            .unwrap();
    }

    true
}

fn clean_old_pending_outpoints(pending: &mut HashMap<TransactionOutpoint, Instant>) {
    let now = Instant::now();
    pending.retain(|_, &mut time| now.duration_since(time) <= Duration::from_secs(3600));
}

/// The relay fee for a transaction of this plan's shape spending `entries`, found by building the
/// same transaction with signature scripts of the exact ML-DSA-87 push sizes (no signing needed:
/// the mass depends on sizes only). The fee is the larger of the compute and storage mass at the
/// relay rate, so it covers whichever the node charges.
fn relay_fee(plan: &TxPlan<'_>, entries: &[UtxoEntry], out_value: u64, payload_size: usize) -> u64 {
    let sig_script = ScriptBuilder::new()
        .add_data(&vec![0u8; MLDSA87_SIG_LEN + 1])
        .expect("sig push fits")
        .add_data(&vec![0u8; plan.key.public_key().len()])
        .expect("pubkey push fits")
        .drain();
    let dummy_outpoint = TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_bytes([0u8; 64]), 0);
    let inputs: Vec<TransactionInput> =
        entries.iter().map(|_| TransactionInput::new(dummy_outpoint, sig_script.clone(), MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
    let outputs: Vec<TransactionOutput> = (0..plan.num_outs).map(|_| TransactionOutput::new(out_value, plan.to_spk.clone())).collect();
    let tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![0u8; payload_size]);
    let mc = mass_calc(plan.params);
    let compute = mc.calc_non_contextual_masses(&tx).compute_mass;
    let storage = mc.calc_contextual_masses(&PopulatedTransaction::new(&tx, entries.to_vec())).map_or(0, |m| m.storage_mass);
    relay_fee_for_compute_mass(compute.max(storage))
}

/// Build and sign a native transaction spending `utxos` into `outputs`, carrying `payload`. This is
/// the same signing path as the wallet's native send (`ValidatorKey`'s ML-DSA-87 sighash under
/// `MLDSA87_TX_CONTEXT`, one signature per input, KIP-9 storage mass committed before signing); it
/// differs only in carrying a payload.
fn build_signed_tx(
    key: &ValidatorKey,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    outputs: Vec<TransactionOutput>,
    payload: Vec<u8>,
    storage_mass_parameter: u64,
) -> Result<Transaction, String> {
    let inputs: Vec<TransactionInput> =
        utxos.iter().map(|(op, _)| TransactionInput::new(*op, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)).collect();
    let entries: Vec<UtxoEntry> = utxos.iter().map(|(_, e)| e.clone()).collect();
    let tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload);
    let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
        .calc_contextual_masses(&PopulatedTransaction::new(&tx, entries.clone()))
        .ok_or_else(|| "contextual mass not computable for the native tx".to_string())?
        .storage_mass;
    tx.set_mass(storage_mass);
    let mtx = MutableTransaction::with_entries(tx, entries);
    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let mut scripts = Vec::with_capacity(utxos.len());
    for i in 0..utxos.len() {
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), i, SIG_HASH_ALL, &reused);
        let mut sig_data = key.sign_with_context(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT).to_vec();
        sig_data.push(SIG_HASH_ALL.to_u8());
        let script = ScriptBuilder::new()
            .add_data(&sig_data)
            .map_err(|e| format!("sig push failed: {e}"))?
            .add_data(key.public_key())
            .map_err(|e| format!("pubkey push failed: {e}"))?
            .drain();
        scripts.push(script);
    }
    let mut tx = mtx.tx;
    for (i, script) in scripts.into_iter().enumerate() {
        tx.inputs[i].signature_script = script;
    }
    Ok(tx)
}

fn select_utxos(
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    min_amount: u64,
    plan: &TxPlan<'_>,
    maximize_utxos: bool,
    next_available_utxo_index: &mut usize,
    tx_config: &TxConfig,
) -> (Vec<(TransactionOutpoint, UtxoEntry)>, u64) {
    const MAX_UTXOS: usize = 8;
    let mut total_amount: u64 = 0;
    let mut selected: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::new();
    let mut rng = rand::thread_rng();

    while *next_available_utxo_index < utxos.len() {
        let (outpoint, entry) = utxos[*next_available_utxo_index].clone();
        total_amount += entry.amount;
        selected.push((outpoint, entry));
        *next_available_utxo_index += 1;

        let entries: Vec<UtxoEntry> = selected.iter().map(|(_, e)| e.clone()).collect();
        let fee = relay_fee(plan, &entries, total_amount / plan.num_outs, tx_config.payload_size);
        let priority_fee = if tx_config.randomize_fee && tx_config.priority_fee > 0 {
            rng.gen_range(0..tx_config.priority_fee)
        } else {
            tx_config.priority_fee
        };
        let fee = fee + priority_fee;

        if total_amount >= min_amount + fee && (!maximize_utxos || selected.len() == MAX_UTXOS) {
            return (selected, total_amount - fee);
        }

        if selected.len() > MAX_UTXOS {
            return (vec![], 0);
        }
    }

    (vec![], 0)
}

/// One payload transaction: `send_amount` split over the plan's outputs (the rest of the inputs is
/// the fee), with `payload_size` random bytes in the payload.
fn generate_tx(
    plan: &TxPlan<'_>,
    utxos: &[(TransactionOutpoint, UtxoEntry)],
    send_amount: u64,
    payload_size: usize,
) -> Result<Transaction, String> {
    let num_outs = plan.num_outs.max(1);
    let per_out = send_amount / num_outs;
    let outputs: Vec<TransactionOutput> = (0..num_outs)
        .map(|i| {
            let value = if i + 1 == num_outs { send_amount - per_out * (num_outs - 1) } else { per_out };
            TransactionOutput { value, script_public_key: plan.to_spk.clone() }
        })
        .collect();
    let mut data = vec![0u8; payload_size];
    rand::thread_rng().fill_bytes(&mut data);
    build_signed_tx(plan.key, utxos, outputs, data, plan.params.storage_mass_parameter)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The payload-size bound stays at the CLI maximum, and a payload below it is accepted by the
    /// shape check (the consensus acceptance itself is exercised on a live devnet, not here).
    #[test]
    fn payload_bound_is_the_cli_maximum() {
        assert_eq!(MAX_PAYLOAD_SIZE, 20_000);
        assert!(17_408 <= MAX_PAYLOAD_SIZE);
    }
}
