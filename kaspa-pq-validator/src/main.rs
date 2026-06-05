//! kaspa-pq-validator — the ADR-0011 single-host validator sidecar.
//!
//! A standalone process that connects to a co-located `kaspad` over a 127.0.0.1 wRPC
//! (borsh) endpoint and, once its stake bond is active, attests to the selected-chain
//! anchor each epoch: it fetches the ready-to-sign target over wRPC, signs it with its
//! ML-DSA-87 validator key (under the equivocation-safety guard), funds a
//! `StakeAttestationShard` transaction from a UTXO at its own address, and submits it.
//! The signing primitives are shared with the in-process `--enable-validator` service via
//! `kaspa-pq-validator-core`.
//!
//! Subcommands: `run` (the validator daemon), `keygen` (generate a validator key), and
//! `status` (one-shot bond/status query). Recommended deployment: `run` beside `kaspad`
//! under systemd (ADR-0011); the node must run `--utxoindex` for the funding lookup.

use clap::{Parser, Subcommand};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::dns_finality::{
    DNS_PAYLOAD_VERSION_V1, SignedEpochCheckOutcome, SignedEpochRecord, StakeAttestation, signature_fingerprint,
    single_attestation_shard,
};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_core::{info, warn};
use kaspa_pq_validator_core::{
    SignedEpochStore, VALIDATOR_SEED_LEN, ValidatorKey, load_validator_seed, parse_stake_bond_ref,
};
use kaspa_rpc_core::{
    GetStakeBondRequest, GetValidatorAttestationTargetRequest, GetValidatorAttestationTargetResponse, RpcTransaction, api::rpc::RpcApi,
};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use rand::RngCore;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Duration;

const VALIDATOR: &str = "kaspa-pq-validator";

/// Kaspa-PQ validator sidecar (ADR-0011).
#[derive(Parser, Debug)]
#[command(name = "kaspa-pq-validator", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the validator daemon: connect to the local node and attest while the bond is active.
    Run(RunArgs),
    /// Generate a new ML-DSA-87 validator key and print its identity + funding address.
    Keygen(KeygenArgs),
    /// One-shot: query the node + bond status and print it.
    Status(StatusArgs),
    /// Stake mined coins: build + submit a StakeBond tx from a UTXO at the funding address.
    Bond(BondArgs),
    /// Begin unbonding a StakeBond: build + submit a signed StakeUnbondRequest for the given
    /// bond outpoint (its locked stake becomes spendable after the unbonding window elapses).
    Unbond(UnbondArgs),
    /// Load generator: continuously spend mature UTXOs at the funding address into fan-out
    /// NATIVE transfers, flooding the node's mempool with valid ML-DSA transactions.
    Spam(SpamArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    /// Local node wRPC (borsh) endpoint, host:port. Bind the node's RPC to 127.0.0.1 only.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,

    /// Stake-bond outpoint backing this validator, "txid_hex:index". Required (together
    /// with --validator-key and --signed-epoch-db) to attest; otherwise observe-only.
    #[arg(long, env = "KASPA_PQ_STAKE_BOND")]
    stake_bond: Option<String>,

    /// Path to the ML-DSA-87 validator signing key (32-byte seed, hex). Required to attest.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: Option<String>,

    /// Path to the persistent equivocation-safety log (JSON). Required to attest — the
    /// guard cannot be enforced across restarts without it. Back this file up.
    #[arg(long, env = "KASPA_PQ_SIGNED_EPOCH_DB")]
    signed_epoch_db: Option<String>,

    /// Compute eligibility + the attestation target and sign it locally, but never submit.
    #[arg(long, env = "KASPA_PQ_DRY_RUN")]
    dry_run: bool,

    /// Expected node network id; refuse to start on mismatch (ADR-0011 §"Same network").
    #[arg(long, env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Seconds between attestation rounds. Each round attests at most the ONE current
    /// canonical-ready epoch, so this poll period must be ≤ an epoch's wall-clock duration
    /// (≈ epoch_length_blocks / blocks-per-second) for a single validator to cover EVERY epoch
    /// and reach the DNS stake-depth threshold. Default 30 suits mainnet (~1 BPS ⇒ ~100 s
    /// epochs); LOWER it on a fast devnet (e.g. 3 at ~9 BPS ⇒ ~11 s epochs) so one validator
    /// keeps up. Revisiting the same epoch within a run is deduped (no re-sign / no rebroadcast),
    /// so a small value only adds cheap RPC polls.
    #[arg(long, default_value_t = 30, env = "KASPA_PQ_ATTEST_POLL_SECS")]
    attest_poll_secs: u64,

    /// Logging level {off, error, warn, info, debug, trace}.
    #[arg(long, default_value = "info", env = "KASPA_PQ_LOG_LEVEL")]
    log_level: String,
}

#[derive(Parser, Debug)]
struct KeygenArgs {
    /// Output path for the validator key (32-byte seed as hex; written with mode 0600 on unix).
    #[arg(long)]
    out: String,

    /// Network for the printed funding address {mainnet, testnet, devnet, simnet}.
    #[arg(long, default_value = "mainnet")]
    network: String,
}

#[derive(Parser, Debug)]
struct StatusArgs {
    /// Local node wRPC (borsh) endpoint, host:port.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,

    /// Stake-bond outpoint to report, "txid_hex:index".
    #[arg(long, env = "KASPA_PQ_STAKE_BOND")]
    stake_bond: Option<String>,
}

#[derive(Parser, Debug)]
struct BondArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,

    /// Path to the ML-DSA-87 validator signing key (32-byte seed, hex). The bond is staked
    /// from a UTXO at this key's own funding address and binds this key as the validator.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// Amount to stake, in sompi. Becomes the bond's locked output-0; must be covered by a
    /// single funding UTXO together with the fee.
    #[arg(long)]
    amount: u64,

    /// First DAA score at which the bond's attestations count. 0 = active as soon as accepted.
    #[arg(long, default_value_t = 0)]
    activation_daa_score: u64,

    /// Per-bond unbonding window in blocks. Must be >= the network's
    /// `unbonding_period_blocks` floor (devnet harness = 700).
    #[arg(long, default_value_t = 700)]
    unbonding_period_blocks: u64,

    /// Fee in sompi for the bond transaction. Default: a mass-based estimate from the network's
    /// mass params (the StakeBond payload carries the 2592-byte pubkey, so the flat attestation
    /// floor is too low to relay). Pass an explicit value to override (e.g. bump under congestion).
    #[arg(long)]
    fee: Option<u64>,

    /// Expected node network id; refuse on mismatch.
    #[arg(long, env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct UnbondArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,

    /// Path to the ML-DSA-87 validator signing key (32-byte seed, hex). Must be the key that
    /// owns the bond (its derived `validator_id` == the bond's `owner_pubkey_hash`), otherwise
    /// the node rejects the unauthorized request.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// The bond to unbond, "txid:index" — the `bond_outpoint` that `bond` printed (i.e. `<txid>:0`).
    #[arg(long)]
    stake_bond: String,

    /// Fee in sompi for the unbond transaction. Default: a mass-based estimate from the network's
    /// mass params (the unbond payload carries the 2592-byte pubkey + 4627-byte sig, so the flat
    /// attestation floor is too low to relay). Pass an explicit value to override.
    #[arg(long)]
    fee: Option<u64>,

    /// Expected node network id; refuse on mismatch.
    #[arg(long, env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct SpamArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,
    /// ML-DSA validator key (32-byte seed, hex) whose funding address holds the coins to spam.
    /// Mine to its `funding_address` first (e.g. `misaminer --wallet <addr>`).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,
    /// Outputs per split tx (fan-out). Each becomes a fresh spendable UTXO, so a chain of these
    /// grows the UTXO set and the tx rate. 2-4 is a good sustained load.
    #[arg(long, default_value_t = 3)]
    fanout: usize,
    /// Flat fee (sompi) per tx; must cover the tx's mass at the relay rate.
    #[arg(long, default_value_t = 50_000)]
    fee: u64,
    /// Max txs to submit per round (per UTXO-set scan).
    #[arg(long, default_value_t = 300)]
    max_per_round: usize,
    /// Milliseconds to sleep between rounds.
    #[arg(long, default_value_t = 200)]
    interval_ms: u64,
    /// Skip UTXOs smaller than this (sompi) — keeps splits above the dust floor.
    #[arg(long, default_value_t = 1_000_000)]
    min_utxo: u64,
    /// Expected node network id; refuse on mismatch.
    #[arg(long, env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Run(args) => {
            kaspa_core::log::init_logger(None, &args.log_level);
            run_daemon(args).await
        }
        Command::Keygen(args) => keygen(args),
        Command::Status(args) => status(args).await,
        Command::Bond(args) => {
            kaspa_core::log::init_logger(None, "info");
            bond(args).await
        }
        Command::Unbond(args) => {
            kaspa_core::log::init_logger(None, "info");
            unbond(args).await
        }
        Command::Spam(args) => {
            kaspa_core::log::init_logger(None, "info");
            spam(args).await
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[{VALIDATOR}] error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Generate a fresh ML-DSA-87 validator key, write the seed to `--out`, and print the
/// derived overlay identity + funding address. The owner / withdrawal key is NOT produced
/// here (ADR-0011 key-separation policy: validator key on the host, owner key off it).
fn keygen(args: KeygenArgs) -> Result<(), String> {
    let prefix = parse_prefix(&args.network)?;
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    rand::thread_rng().fill_bytes(&mut seed);
    let key = ValidatorKey::from_seed(seed);

    let mut hex_buf = [0u8; VALIDATOR_SEED_LEN * 2];
    faster_hex::hex_encode(&seed, &mut hex_buf).map_err(|e| format!("hex encode failed: {e}"))?;
    let hex = std::str::from_utf8(&hex_buf).expect("hex is valid utf-8");

    std::fs::write(&args.out, hex).map_err(|e| format!("cannot write key to '{}': {e}", args.out))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&args.out, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot chmod 600 '{}': {e}", args.out))?;
    }

    println!("validator key written to {} (keep it secret; back it up; do NOT run it on a second host)", args.out);
    println!("validator_id:    {}", key.validator_id);
    println!("funding_address: {}", key.funding_address(prefix));
    Ok(())
}

/// One-shot status report: connect, print the node's network/sync state, and (if a bond is
/// given) the bond's effective status. Useful for `systemctl`-free health checks.
async fn status(args: StatusArgs) -> Result<(), String> {
    kaspa_core::log::init_logger(None, "warn");
    let client = connect(&args.node_rpc).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    println!("node_network: {}", server.network_id);
    println!("node_synced:  {}", server.is_synced);
    println!("node_version: {}", server.server_version);
    if let Some(bond) = &args.stake_bond {
        match client.get_stake_bond(GetStakeBondRequest { bond_outpoint: bond.clone() }).await {
            Ok(b) if b.available => {
                println!("bond:         {bond}");
                println!("bond_status:  {}", b.effective_status);
                println!("bond_amount:  {}", b.amount);
                println!("validator_id: {}", b.validator_id);
            }
            Ok(_) => println!("bond:         {bond} (not found in the registry)"),
            Err(e) => println!("bond:         query failed: {e} (does the node configure the overlay?)"),
        }
    }
    let _ = client.disconnect().await;
    Ok(())
}

/// Stake mined coins into a new bond: load the validator key, find a funding UTXO at its own
/// address, build a signed `StakeBond` tx (locked output-0 == amount, change back to self),
/// submit it, and print the resulting `bond_outpoint` (`txid:0`) to pass to `run --stake-bond`.
async fn bond(args: BondArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let client = connect(&args.node_rpc).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref() {
        if node_network != expected {
            return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
        }
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let mass_calc =
        MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op, params.storage_mass_parameter);
    // Mass-based fee unless overridden: the StakeBond payload carries the 2592-byte validator
    // pubkey, so the flat attestation floor is far below the mempool minimum (live finding 2026-06-04).
    let fee = match args.fee {
        Some(f) => f,
        None => key.estimate_bond_fee(&mass_calc, prefix),
    };
    info!(
        "[{VALIDATOR}] staking {} sompi (fee {fee} sompi{}) as validator_id={} (funding {})",
        args.amount,
        if args.fee.is_some() { "" } else { ", mass-based" },
        key.validator_id,
        funding_addr
    );

    // Need a single MATURE UTXO covering amount + fee. Pick the largest available (most likely to
    // fit). A coinbase UTXO is unspendable until `coinbase_maturity` blocks have passed
    // (consensus rule); a miner still paying this funding address mints a fresh (immature)
    // coinbase every block, so without this filter the "largest" pick is almost always the
    // newest = immature one, and the bond tx is rejected forever ("spends an immature UTXO").
    // Filtering by maturity here makes `bond` succeed even while the funding miner keeps running.
    let needed = args.amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    let utxos = client
        .get_utxos_by_addresses(vec![funding_addr.clone()])
        .await
        .map_err(|e| format!("getUtxosByAddresses failed (does the node run --utxoindex?): {e}"))?;
    let funding = utxos
        .into_iter()
        .filter(|e| e.utxo_entry.amount >= needed)
        .filter(|e| is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, virtual_daa, coinbase_maturity))
        .max_by_key(|e| e.utxo_entry.amount)
        .ok_or_else(|| format!("no single MATURE funding UTXO >= {needed} sompi (amount+fee) at {funding_addr}; \
            mine/send funds there and wait for coinbase maturity ({coinbase_maturity} blocks)"))?;
    let funding_outpoint: TransactionOutpoint = funding.outpoint.into();
    let funding_entry: UtxoEntry = funding.utxo_entry.into();

    let tx = key.build_funded_stake_bond_tx(
        args.amount,
        args.activation_daa_score,
        args.unbonding_period_blocks,
        key.reward_spk_payload(),
        funding_outpoint,
        &funding_entry,
        fee,
    )?;

    let txid = client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
    info!("[{VALIDATOR}] submitted stake-bond tx (txid={txid})");
    // The bond outpoint is always output-0 of the bond tx.
    println!("bond_outpoint: {txid}:0");
    println!("(once accepted + activation_daa_score reached, run: {VALIDATOR} run --validator-key <key> --stake-bond {txid}:0 --signed-epoch-db <db>)");
    let _ = client.disconnect().await;
    Ok(())
}

/// Begin unbonding a `StakeBond`: load the validator key, find a single MATURE funding UTXO at its
/// funding address (NOT the bond's own locked output-0), build a signed `StakeUnbondRequest` for
/// `--stake-bond`, submit it, and print the result. After acceptance the bond enters `Unbonding`;
/// its locked stake becomes spendable once `unbonding_period_blocks` further blocks elapse.
async fn unbond(args: UnbondArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let bond_outpoint = parse_stake_bond_ref(&args.stake_bond)?;
    let client = connect(&args.node_rpc).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref() {
        if node_network != expected {
            return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
        }
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let mass_calc =
        MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op, params.storage_mass_parameter);
    // Mass-based fee unless overridden (the unbond payload carries the 2592-byte pubkey + 4627-byte sig).
    let fee = match args.fee {
        Some(f) => f,
        None => key.estimate_unbond_fee(&mass_calc, prefix),
    };
    info!(
        "[{VALIDATOR}] unbonding {bond_outpoint} (fee {fee} sompi{}) for validator_id={} (funding {funding_addr})",
        if args.fee.is_some() { "" } else { ", mass-based" },
        key.validator_id
    );

    // Need a single MATURE UTXO that covers the fee — and it must NOT be the bond's own locked
    // output-0: the consensus bond-spend-gate keeps that locked until release, so trying to pay the
    // fee from it would be rejected. Coinbase maturity is filtered for the same reason as `bond`
    // (a miner still paying this address mints a fresh immature coinbase every block).
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    let utxos = client
        .get_utxos_by_addresses(vec![funding_addr.clone()])
        .await
        .map_err(|e| format!("getUtxosByAddresses failed (does the node run --utxoindex?): {e}"))?;
    let funding = utxos
        .into_iter()
        .filter(|e| TransactionOutpoint::from(e.outpoint.clone()) != bond_outpoint)
        .filter(|e| e.utxo_entry.amount > fee)
        .filter(|e| is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, virtual_daa, coinbase_maturity))
        .max_by_key(|e| e.utxo_entry.amount)
        .ok_or_else(|| format!("no single MATURE funding UTXO > {} sompi (fee) at {funding_addr} other than the bond itself; \
            send funds there and wait for coinbase maturity ({coinbase_maturity} blocks)", fee))?;
    let funding_outpoint: TransactionOutpoint = funding.outpoint.into();
    let funding_entry: UtxoEntry = funding.utxo_entry.into();

    // audit M-04: bind the unbond authorization to this network's genesis hash (prevents replay
    // of the signed authorization on another network).
    let tx = key.build_funded_unbond_tx(params.genesis.hash.as_byte_slice(), bond_outpoint, funding_outpoint, &funding_entry, fee)?;

    let txid = client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
    info!("[{VALIDATOR}] submitted unbond request (txid={txid}) for bond {bond_outpoint}");
    println!("unbond_request_txid: {txid}");
    println!("(once accepted the bond enters Unbonding; its locked stake is spendable after unbonding_period_blocks more blocks)");
    let _ = client.disconnect().await;
    Ok(())
}

/// Load generator (devnet stress): continuously scan mature UTXOs at the key's funding address
/// and spend each into a fan-out NATIVE transfer back to self, flooding the node's mempool with
/// valid ML-DSA transactions. Each fan-out output becomes a fresh spendable UTXO, so the UTXO
/// set (and the tx rate) grows until the mempool saturates. Submit errors (mempool full, already
/// spent, orphan) are expected under load and ignored. Runs until killed.
async fn spam(args: SpamArgs) -> Result<(), String> {
    let key = ValidatorKey::from_seed(load_validator_seed(&args.validator_key)?);
    let client = connect(&args.node_rpc).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref() {
        if node_network != expected {
            return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
        }
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let coinbase_maturity = params.coinbase_maturity();
    let storage_mass_parameter = params.storage_mass_parameter;
    info!(
        "[{VALIDATOR}] SPAM: flooding {node_network} from {funding_addr} (fanout={}, fee={}, interval={}ms). Fund it via `misaminer --wallet {funding_addr}`.",
        args.fanout, args.fee, args.interval_ms
    );

    let mut total: u64 = 0;
    loop {
        let virtual_daa = client.get_server_info().await.map(|s| s.virtual_daa_score).unwrap_or(0);
        let utxos = match client.get_utxos_by_addresses(vec![funding_addr.clone()]).await {
            Ok(u) => u,
            Err(e) => {
                warn!("[{VALIDATOR}] SPAM: getUtxosByAddresses failed: {e}");
                tokio::time::sleep(Duration::from_millis(args.interval_ms)).await;
                continue;
            }
        };
        let mut spendable: Vec<_> = utxos
            .into_iter()
            .filter(|e| e.utxo_entry.amount >= args.min_utxo)
            .filter(|e| is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, virtual_daa, coinbase_maturity))
            .collect();
        spendable.sort_by_key(|e| std::cmp::Reverse(e.utxo_entry.amount));

        let mut round = 0u64;
        for e in spendable.into_iter().take(args.max_per_round) {
            let funding_outpoint: TransactionOutpoint = e.outpoint.into();
            let funding_entry: UtxoEntry = e.utxo_entry.into();
            let Ok(tx) = key.build_funded_split_tx(funding_outpoint, &funding_entry, args.fee, args.fanout, storage_mass_parameter)
            else {
                continue;
            };
            if client.submit_transaction(RpcTransaction::from(&tx), false).await.is_ok() {
                round += 1;
                total += 1;
            }
        }
        if round > 0 {
            info!("[{VALIDATOR}] SPAM: +{round} txs this round (total {total}, vDAA {virtual_daa})");
        }
        tokio::time::sleep(Duration::from_millis(args.interval_ms)).await;
    }
}

async fn connect(node_rpc: &str) -> Result<KaspaRpcClient, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| format!("failed to build wRPC client: {e}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| format!("failed to connect to node {url}: {e}"))?;
    Ok(client)
}

async fn run_daemon(args: RunArgs) -> Result<(), String> {
    info!("[{VALIDATOR}] connecting to local node at ws://{} (dry_run={})", args.node_rpc, args.dry_run);
    let client = connect(&args.node_rpc).await?;

    // Network-id guard (ADR-0011 §"Same network"): never attest against the wrong net.
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    match args.network.as_deref() {
        Some(expected) if node_network != expected => {
            return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
        }
        _ => {}
    }
    let prefix = prefix_for(server.network_id.network_type);
    let params = Params::from(server.network_id);
    let coinbase_maturity = params.coinbase_maturity();
    let mass_calc =
        MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op, params.storage_mass_parameter);
    info!("[{VALIDATOR}] connected: network={node_network} synced={} version={}", server.is_synced, server.server_version);

    // Load the signing identity if fully configured (key + bond + state DB); else observe.
    let attestor = Attestor::load(&args, prefix, coinbase_maturity, &mass_calc)?;
    match &attestor {
        Some(a) => info!(
            "[{VALIDATOR}] attesting as validator_id={} (funding {}, fee {} sompi mass-based)",
            a.key.validator_id,
            a.key.funding_address(prefix),
            a.attestation_fee
        ),
        None => info!("[{VALIDATOR}] observe-only (need --validator-key + --stake-bond + --signed-epoch-db to attest)"),
    }

    // ADR-0011 §"Auto-startup ordering": tolerate every "not yet" state, loop until shutdown.
    let result = tokio::select! {
        r = run_loop(&client, &args, attestor) => r,
        _ = tokio::signal::ctrl_c() => {
            info!("[{VALIDATOR}] shutdown signal received");
            Ok(())
        }
    };
    let _ = client.disconnect().await;
    result
}

/// The ML-DSA-87 signing identity + equivocation guard, present only when fully
/// configured. Shares its primitives with the in-process service via
/// `kaspa-pq-validator-core`.
struct Attestor {
    key: ValidatorKey,
    bond_outpoint: TransactionOutpoint,
    signed_store: SignedEpochStore,
    prefix: Prefix,
    /// Network coinbase-maturity (blocks); a coinbase funding UTXO younger than this cannot be
    /// spent for the attestation tx. Captured once at load from the node's network id.
    coinbase_maturity: u64,
    /// Mass-based attestation-shard fee (sompi), computed once at load from the network's mass
    /// params (the shard tx shape is fixed, so the fee is constant across epochs). Replaces the
    /// flat floor, which is ~10× below the kaspa-pq mempool minimum for this payload-heavy tx.
    attestation_fee: u64,
    /// The last epoch this PROCESS has already attested (submitted a shard for). Lets a short
    /// `--attest-poll-secs` revisit the same canonical-ready epoch cheaply without re-signing or
    /// rebroadcasting (which would burn a funding UTXO each poll). Reset on restart, so the
    /// persistent `SignedEpochStore` still drives a single crash-recovery rebroadcast.
    last_attested_epoch: Option<u64>,
}

impl Attestor {
    /// Load the signing identity iff `--validator-key`, `--stake-bond` and
    /// `--signed-epoch-db` are all provided. The state file is rejected if it belongs to a
    /// different validator/bond (cross-key equivocation guard).
    fn load(args: &RunArgs, prefix: Prefix, coinbase_maturity: u64, mass_calc: &MassCalculator) -> Result<Option<Self>, String> {
        let (Some(key_path), Some(bond_ref), Some(db)) = (&args.validator_key, &args.stake_bond, &args.signed_epoch_db) else {
            return Ok(None);
        };
        let key = ValidatorKey::from_seed(load_validator_seed(key_path)?);
        let bond_outpoint = parse_stake_bond_ref(bond_ref)?;
        let signed_store = SignedEpochStore::load_or_empty(db.into(), key.validator_id, bond_outpoint)?;
        let attestation_fee = key.estimate_attestation_fee(mass_calc, prefix);
        Ok(Some(Self { key, bond_outpoint, signed_store, prefix, coinbase_maturity, attestation_fee, last_attested_epoch: None }))
    }

    /// Sign the attestation `target` under the equivocation guard and (unless `dry_run`)
    /// fund + submit the `StakeAttestationShard` transaction. Returns `Err` only on a
    /// genuine failure (self-verify, funding, build, submit); the benign "already attested
    /// this epoch" path logs and returns `Ok`.
    async fn attest(
        &mut self,
        client: &KaspaRpcClient,
        target: &GetValidatorAttestationTargetResponse,
        dry_run: bool,
        virtual_daa: u64,
    ) -> Result<(), String> {
        let message = decode_message(&target.message)?;
        let target_hash = parse_hash64(&target.target_hash)?;
        let vsc = parse_hash64(&target.validator_set_commitment)?;

        // Sign + local self-verify (the same check the consensus aggregator runs).
        let signature = self.key.sign_attestation(&message);
        if !self.key.verify_attestation(&message, &signature) {
            return Err("local attestation self-verify failed".to_string());
        }
        let record = SignedEpochRecord {
            epoch: target.epoch,
            target_hash,
            target_daa_score: target.target_daa_score,
            signature_fingerprint: signature_fingerprint(&signature),
        };

        // ADR-0011 equivocation guard.
        let outcome = self.signed_store.check(&record);
        match outcome {
            SignedEpochCheckOutcome::Block => {
                // One key signs at most one target per epoch; once it has committed to the
                // first anchor it saw this epoch, a later (moved-sink) target is refused.
                info!("[{VALIDATOR}] already attested epoch {} (target moved); skipping", target.epoch);
                return Ok(());
            }
            SignedEpochCheckOutcome::Allow | SignedEpochCheckOutcome::AllowRebroadcast => {}
        }

        if dry_run {
            info!("[{VALIDATOR}] DRY-RUN signed epoch {} target={} (not submitting)", target.epoch, target.target_hash);
            return Ok(());
        }

        // Build the attestation shard.
        let att = StakeAttestation {
            version: DNS_PAYLOAD_VERSION_V1,
            validator_id: self.key.validator_id,
            bond_outpoint: self.bond_outpoint,
            epoch: target.epoch,
            target_hash,
            target_daa_score: target.target_daa_score,
            validator_set_commitment: vsc,
            signature: signature.to_vec(),
        };
        let shard = single_attestation_shard(att);

        // Find a funding UTXO at the validator's own P2PKH-ML-DSA address (needs node
        // --utxoindex). Funding model A: a small input pays the fee, change returns to self.
        let fee = self.attestation_fee;
        let funding_addr = self.key.funding_address(self.prefix);
        let utxos = client
            .get_utxos_by_addresses(vec![funding_addr])
            .await
            .map_err(|e| format!("getUtxosByAddresses failed (does the node run --utxoindex?): {e}"))?;
        // Skip immature coinbase UTXOs (consensus coinbase-maturity rule): a funding miner mints a
        // fresh coinbase each block, so an unfiltered pick keeps grabbing the newest=immature one
        // and the shard tx is rejected ("spends an immature UTXO"). Prefer a mature one.
        let funding = utxos
            .into_iter()
            .filter(|e| e.utxo_entry.amount > fee)
            .find(|e| is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, virtual_daa, self.coinbase_maturity))
            .ok_or_else(|| format!("no MATURE funding UTXO > {fee} sompi at the validator funding address; \
                send funds there and wait for coinbase maturity ({} blocks)", self.coinbase_maturity))?;
        let funding_outpoint: TransactionOutpoint = funding.outpoint.into();
        let funding_entry: UtxoEntry = funding.utxo_entry.into();

        let tx = self.key.build_funded_shard_tx(&shard, funding_outpoint, &funding_entry, fee)?;

        // Persist the signing record BEFORE broadcasting, so a crash post-submit cannot lose
        // the record and let a restart sign a different target for this epoch.
        if outcome == SignedEpochCheckOutcome::Allow {
            self.signed_store.record_and_flush(record)?;
        }

        let txid =
            client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
        info!("[{VALIDATOR}] submitted attestation shard for epoch {} (txid={txid})", target.epoch);
        Ok(())
    }
}

/// The ADR-0011 validator runtime loop. Returns `Err` only on the fatal `Slashed` state;
/// every other state sleeps and retries.
async fn run_loop(client: &KaspaRpcClient, args: &RunArgs, mut attestor: Option<Attestor>) -> Result<(), String> {
    loop {
        // 1. Sync guard (NodeNotSynced).
        let server = match client.get_server_info().await {
            Ok(s) => s,
            Err(e) => {
                warn!("[{VALIDATOR}] getServerInfo failed: {e}; retrying");
                sleep_secs(5).await;
                continue;
            }
        };
        if !server.is_synced {
            info!("[{VALIDATOR}] status=NodeNotSynced (virtual_daa={})", server.virtual_daa_score);
            sleep_secs(5).await;
            continue;
        }

        // 2. Bond configured?
        let Some(bond) = args.stake_bond.as_deref() else {
            info!("[{VALIDATOR}] status=Idle (no --stake-bond configured; observing only)");
            sleep_secs(30).await;
            continue;
        };

        // 3. Bond lifecycle (ADR-0011 state machine).
        let bond_resp = match client.get_stake_bond(GetStakeBondRequest { bond_outpoint: bond.to_owned() }).await {
            Ok(r) => r,
            Err(e) => {
                warn!("[{VALIDATOR}] getStakeBond failed: {e}; retrying");
                sleep_secs(15).await;
                continue;
            }
        };
        if !bond_resp.available {
            info!("[{VALIDATOR}] status=BondNotFound (bond {bond} not in the registry yet)");
            sleep_secs(30).await;
            continue;
        }
        match bond_resp.effective_status.as_str() {
            "pending" => {
                info!("[{VALIDATOR}] status=BondPending (activation_daa={})", bond_resp.activation_daa_score);
                sleep_secs(60).await;
            }
            "unbonding" => {
                warn!("[{VALIDATOR}] status=Unbonding; will stop attesting once finalised");
                sleep_secs(60).await;
            }
            "slashed" => {
                return Err(format!("status=Slashed: bond {bond} has been slashed (fatal)"));
            }
            "active" => {
                // ADR-0017: every active-bond validator attests. Fetch the ready-to-sign
                // target, then sign + (unless dry-run / observe-only) fund + submit.
                match client
                    .get_validator_attestation_target(GetValidatorAttestationTargetRequest { bond_outpoint: bond.to_owned() })
                    .await
                {
                    Ok(t) if t.available => match &mut attestor {
                        // Already attested this epoch this run: a short --attest-poll-secs revisits the
                        // same canonical-ready epoch until it advances; skip cheaply (no re-sign / no
                        // rebroadcast) so fast polling doesn't burn a funding UTXO per round.
                        Some(a) if a.last_attested_epoch == Some(t.epoch) => {}
                        Some(a) => match a.attest(client, &t, args.dry_run, server.virtual_daa_score).await {
                            Ok(()) => a.last_attested_epoch = Some(t.epoch),
                            Err(e) => warn!("[{VALIDATOR}] attest failed for epoch {}: {e}", t.epoch),
                        },
                        None => info!(
                            "[{VALIDATOR}] status=ActiveEligible epoch={} target={} (observe-only; not signing)",
                            t.epoch, t.target_hash
                        ),
                    },
                    Ok(_) => info!("[{VALIDATOR}] status=ActiveIdle (no attestation target available this tick)"),
                    Err(e) => warn!("[{VALIDATOR}] getValidatorAttestationTarget failed: {e}"),
                }
                sleep_secs(args.attest_poll_secs).await;
            }
            other => {
                warn!("[{VALIDATOR}] unknown bond status '{other}'; retrying");
                sleep_secs(30).await;
            }
        }
    }
}

/// Whether a funding UTXO can be spent right now. A coinbase output is locked until
/// `coinbase_maturity` blocks have passed since it was mined (consensus rule); a non-coinbase
/// output is always spendable. `virtual_daa` is the node's current virtual DAA score.
/// Saturating so a (transient) `block_daa_score > virtual_daa` reads as "not yet mature".
/// Takes the two raw fields (not a typed entry) so it works for both `UtxoEntry` and the
/// RPC `RpcUtxoEntry` returned by `get_utxos_by_addresses` (same fields, different type).
fn is_spendable(is_coinbase: bool, block_daa_score: u64, virtual_daa: u64, coinbase_maturity: u64) -> bool {
    if !is_coinbase {
        return true;
    }
    virtual_daa.saturating_sub(block_daa_score) >= coinbase_maturity
}

/// Map the node's `NetworkType` to the bech32 address `Prefix` (for the funding address).
fn prefix_for(network_type: NetworkType) -> Prefix {
    match network_type {
        NetworkType::Mainnet => Prefix::Mainnet,
        NetworkType::Testnet => Prefix::Testnet,
        NetworkType::Devnet => Prefix::Devnet,
        NetworkType::Simnet => Prefix::Simnet,
    }
}

/// Parse a network name {mainnet, testnet, devnet, simnet} to its address `Prefix`.
fn parse_prefix(s: &str) -> Result<Prefix, String> {
    match s.to_ascii_lowercase().as_str() {
        "mainnet" => Ok(Prefix::Mainnet),
        "testnet" => Ok(Prefix::Testnet),
        "devnet" => Ok(Prefix::Devnet),
        "simnet" => Ok(Prefix::Simnet),
        other => Err(format!("unknown network '{other}' (expected mainnet/testnet/devnet/simnet)")),
    }
}

/// Decode the 32-byte ready-to-sign attestation message digest (hex).
fn decode_message(hex: &str) -> Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    faster_hex::hex_decode(hex.as_bytes(), &mut out).map_err(|e| format!("bad attestation message hex '{hex}': {e}"))?;
    Ok(out)
}

/// Parse a 64-byte Hash64 from hex (128 chars).
fn parse_hash64(hex: &str) -> Result<Hash64, String> {
    Hash64::from_str(hex).map_err(|e| format!("bad Hash64 hex '{hex}': {e}"))
}

async fn sleep_secs(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefix_known_and_unknown() {
        assert_eq!(parse_prefix("mainnet").unwrap(), Prefix::Mainnet);
        assert_eq!(parse_prefix("SIMNET").unwrap(), Prefix::Simnet);
        assert!(parse_prefix("bogus").is_err());
    }

    #[test]
    fn prefix_for_maps_every_network() {
        assert_eq!(prefix_for(NetworkType::Mainnet), Prefix::Mainnet);
        assert_eq!(prefix_for(NetworkType::Testnet), Prefix::Testnet);
        assert_eq!(prefix_for(NetworkType::Devnet), Prefix::Devnet);
        assert_eq!(prefix_for(NetworkType::Simnet), Prefix::Simnet);
    }

    #[test]
    fn is_spendable_respects_coinbase_maturity() {
        let maturity = 1000;
        // coinbase mined at daa 5000: needs virtual_daa - 5000 >= 1000
        assert!(!is_spendable(true, 5000, 5500, maturity), "depth 500 < 1000 → immature");
        assert!(!is_spendable(true, 5000, 5999, maturity), "depth 999 < 1000 → immature");
        assert!(is_spendable(true, 5000, 6000, maturity), "depth exactly 1000 → mature");
        assert!(is_spendable(true, 5000, 9000, maturity), "depth 4000 → mature");
        // a coinbase from the future (transient reorg view) reads as not-yet-mature, never panics
        assert!(!is_spendable(true, 6000, 5000, maturity));
        // non-coinbase outputs are always spendable regardless of age
        assert!(is_spendable(false, 5999, 6000, maturity));
        assert!(is_spendable(false, 6000, 6000, maturity));
    }

    #[test]
    fn decode_message_roundtrip_and_reject() {
        let bytes = [0xABu8; 32];
        let mut hex = [0u8; 64];
        faster_hex::hex_encode(&bytes, &mut hex).unwrap();
        let decoded = decode_message(std::str::from_utf8(&hex).unwrap()).unwrap();
        assert_eq!(decoded, bytes);
        assert!(decode_message("zz").is_err());
    }
}
