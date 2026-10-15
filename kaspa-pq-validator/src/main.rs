//! PALW key, funding and historical stake-bond exit tools. DNS voting is retired.

use clap::{Parser, Subcommand};
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::network::{EndpointKind, NetworkId, NetworkType};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_core::{info, warn};
use kaspa_pq_validator_core::{VALIDATOR_SEED_LEN, ValidatorKey, is_spendable, load_validator_seed, parse_stake_bond_ref};
use kaspa_rpc_core::{GetStakeBondRequest, RpcTransaction, api::rpc::RpcApi};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use rand::RngCore;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Duration;

const VALIDATOR: &str = "kaspa-pq-validator";

/// PALW key and funding utility (legacy ADR-0011).
#[derive(Parser, Debug)]
#[command(name = "kaspa-pq-validator", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate a new ML-DSA-87 validator key and print its identity + funding address.
    Keygen(KeygenArgs),
    /// One-shot: query the node + bond status and print it.
    Status(StatusArgs),
    /// Begin unbonding a StakeBond: build + submit a signed StakeUnbondRequest for the given
    /// bond outpoint (its locked stake becomes spendable after the unbonding window elapses).
    Unbond(UnbondArgs),
    /// Load generator: continuously spend mature UTXOs at the funding address into fan-out
    /// NATIVE transfers, flooding the node's mempool with valid ML-DSA transactions.
    Spam(SpamArgs),
    /// One-shot headless balance: query the node's `getBalancesByAddresses` for one or more
    /// `misaka:`/`misakatest:` addresses over wRPC and print each balance, then exit (no
    /// interactive wallet needed). The node must run --utxoindex.
    Balance(BalanceArgs),
}

#[derive(Parser, Debug)]
struct KeygenArgs {
    /// Output path for the validator key (32-byte seed as hex; written with mode 0600 on unix).
    #[arg(long)]
    out: String,

    /// Network for the printed funding address {mainnet, testnet, devnet, simnet}.
    /// Reads KASPA_PQ_NETWORK so `misaka validator keygen` inherits the unified CLI's
    /// --network-id (an explicit --network/--network-id still wins).
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK", default_value = "mainnet")]
    network: String,
}

#[derive(Parser, Debug)]
struct StatusArgs {
    /// Local node wRPC (borsh) endpoint, host:port.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Network id; used to auto-resolve the node endpoint when --node-wrpc-borsh is omitted.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Stake-bond outpoint to report, "txid_hex:index".
    #[arg(long, env = "KASPA_PQ_STAKE_BOND")]
    stake_bond: Option<String>,
}

#[derive(Parser, Debug)]
struct UnbondArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

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
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct SpamArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
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
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[derive(Parser, Debug)]
struct BalanceArgs {
    /// Node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,
    /// Address to query, e.g. `misakatest:q...`. Repeat --address for several (one RPC call).
    #[arg(long, required = true)]
    address: Vec<String>,
    /// Expected node network id (e.g. `testnet-10`); refuse on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Keygen(args) => keygen(args),
        Command::Status(args) => status(args).await,
        Command::Unbond(args) => {
            kaspa_core::log::init_logger(None, "info");
            unbond(args).await
        }
        Command::Spam(args) => {
            kaspa_core::log::init_logger(None, "info");
            spam(args).await
        }
        Command::Balance(args) => {
            kaspa_core::log::init_logger(None, "info");
            balance(args).await
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

    // Create the key file atomically and refuse to clobber an existing one. `create_new`
    // (O_CREAT|O_EXCL) both prevents silently destroying a funded validator's key on a mistyped path
    // and rejects following a pre-planted symlink; `.mode(0600)` sets owner-only perms at creation, so
    // there is never the group/world-readable window a write-then-chmod sequence leaves open.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&args.out)
            .map_err(|e| format!("cannot create key file '{}' (it must not already exist): {e}", args.out))?;
        f.write_all(hex.as_bytes()).map_err(|e| format!("cannot write key to '{}': {e}", args.out))?;
        f.sync_all().map_err(|e| format!("cannot fsync key file '{}': {e}", args.out))?;
    }
    #[cfg(not(unix))]
    {
        if std::path::Path::new(&args.out).exists() {
            return Err(format!("refusing to overwrite existing key file '{}'", args.out));
        }
        std::fs::write(&args.out, hex).map_err(|e| format!("cannot write key to '{}': {e}", args.out))?;
    }

    // Best-effort scrub of the in-memory seed/hex material (black_box discourages dead-store removal).
    seed.fill(0);
    hex_buf.fill(0);
    std::hint::black_box(&seed);
    std::hint::black_box(&hex_buf);

    println!("validator key written to {} (keep it secret; back it up; do NOT run it on a second host)", args.out);
    println!("validator_id:    {}", key.validator_id);
    println!("funding_address: {}", key.funding_address(prefix));
    Ok(())
}

/// One-shot status report: connect, print the node's network/sync state, and (if a bond is
/// given) the bond's effective status. Useful for `systemctl`-free health checks.
async fn status(args: StatusArgs) -> Result<(), String> {
    kaspa_core::log::init_logger(None, "warn");
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
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
    match client.get_dns_confirmation().await {
        Ok(d) if d.available => {
            let health = match d.health {
                0 => "DisabledBeforeActivation",
                1 => "Active",
                2 => "DegradedStakeQualityLow",
                3 => "DegradedCertificateCensored",
                _ => "Unknown",
            };
            println!("dns_confirmed: {}", d.dns_confirmed);
            println!("pow_confirmed: {}", d.pow_confirmed);
            println!("work_depth:    {}/{}", d.work_depth, d.required_work_depth);
            println!("stake_depth:   {}/{}", d.stake_depth, d.required_stake_depth);
            println!("dns_health:    {health}");
            println!("dns_anchor:    {} (daa {})", d.last_dns_confirmed_anchor, d.last_dns_confirmed_anchor_daa_score);
        }
        Ok(_) => println!("dns:          overlay not active on this node"),
        Err(e) => println!("dns:          query failed: {e}"),
    }
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
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    let mass_calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );
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
    // The EFFECTIVE spend maturity (floor ∨ ADR-0018 settlement) — the floor alone selects
    // coinbases the node refuses (issue #81).
    let coinbase_maturity = params.coinbase_spend_maturity();
    let virtual_daa = server.virtual_daa_score;
    let (mut candidates, mature_seen) =
        top_mature_funding_paged(&client, &funding_addr, virtual_daa, coinbase_maturity, 1, Some(bond_outpoint)).await?;
    let funding = candidates.pop().filter(|e| e.entry.amount > fee).ok_or_else(|| {
        format!(
            "no single MATURE funding UTXO > {} sompi (fee) at {funding_addr} other than the bond itself \
            ({} mature scanned); \
            send funds there and wait for coinbase maturity ({coinbase_maturity} blocks)",
            fee, mature_seen
        )
    })?;
    let funding_outpoint = funding.outpoint;
    let funding_entry = funding.entry;

    // audit M-04: bind the unbond authorization to this network's genesis hash (prevents replay
    // of the signed authorization on another network).
    let tx = key.build_funded_unbond_tx(params.genesis.hash.as_byte_slice(), bond_outpoint, funding_outpoint, &funding_entry, fee)?;

    let txid =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
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
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_addr = key.funding_address(prefix);
    let params = Params::from(server.network_id);
    // The EFFECTIVE spend maturity (floor ∨ ADR-0018 settlement) — the floor alone selects
    // coinbases the node refuses (issue #81).
    let coinbase_maturity = params.coinbase_spend_maturity();
    let storage_mass_parameter = params.storage_mass_parameter;
    info!(
        "[{VALIDATOR}] SPAM: flooding {node_network} from {funding_addr} (fanout={}, fee={}, interval={}ms). Fund it via `misaminer --wallet {funding_addr}`.",
        args.fanout, args.fee, args.interval_ms
    );

    let mut total: u64 = 0;
    loop {
        let virtual_daa = client.get_server_info().await.map(|s| s.virtual_daa_score).unwrap_or(0);
        let spendable =
            match top_mature_funding_paged(&client, &funding_addr, virtual_daa, coinbase_maturity, args.max_per_round, None).await {
                Ok((u, _)) => u,
                Err(e) => {
                    warn!("[{VALIDATOR}] SPAM: getUtxosByAddressPage failed: {e}");
                    tokio::time::sleep(Duration::from_millis(args.interval_ms)).await;
                    continue;
                }
            };

        let mut round = 0u64;
        for e in spendable.into_iter().filter(|e| e.entry.amount >= args.min_utxo).take(args.max_per_round) {
            let funding_outpoint = e.outpoint;
            let funding_entry = e.entry;
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

/// kaspa-pq: one-shot headless balance. Resolves each address, queries the node's
/// `getBalancesByAddresses` (requires --utxoindex), and prints `address <sompi> <BILI> BILI` to
/// STDOUT — one tab-separated line per address (plus a TOTAL line for several) — then exits.
/// Connection / sync notes go to the log so STDOUT stays clean for scripting
/// (e.g. `kaspa-pq-validator balance --address misakatest:q... | awk '{print $2}'`).
async fn balance(args: BalanceArgs) -> Result<(), String> {
    let mut addrs = Vec::with_capacity(args.address.len());
    for a in &args.address {
        addrs.push(Address::try_from(a.as_str()).map_err(|e| format!("invalid address '{a}': {e}"))?);
    }
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        let _ = client.disconnect().await;
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    if !server.has_utxo_index {
        let _ = client.disconnect().await;
        return Err(format!("node '{node_network}' has no UTXO index — restart kaspad with --utxoindex"));
    }
    if !server.is_synced {
        info!("[{VALIDATOR}] WARNING: node '{node_network}' is NOT fully synced — balance may be stale");
    }
    let entries = client.get_balances_by_addresses(addrs.clone()).await.map_err(|e| format!("getBalancesByAddresses failed: {e}"))?;
    // Map the response back by address string (response order is not guaranteed).
    let found: std::collections::HashMap<String, u64> =
        entries.into_iter().map(|e| (e.address.to_string(), e.balance.unwrap_or(0))).collect();
    let mut total = 0u64;
    for a in &addrs {
        let bal = found.get(&a.to_string()).copied().unwrap_or(0);
        total = total.saturating_add(bal);
        println!("{a}\t{bal}\t{} BILI", format_msk(bal));
    }
    if addrs.len() > 1 {
        println!("TOTAL\t{total}\t{} BILI", format_msk(total));
    }
    let _ = client.disconnect().await;
    Ok(())
}

/// Format a sompi amount as BILI for display (L1 = 8 decimals; 1 BILI = 100_000_000 sompi).
fn format_msk(sompi: u64) -> String {
    format!("{}.{:08}", sompi / 100_000_000, sompi % 100_000_000)
}

/// On a connection failure, hint when the endpoint port looks like a DIFFERENT service
/// than the wRPC Borsh the validator needs (design §10.1) — the most common operator
/// mistake is pointing the validator at node gRPC / wRPC JSON / the EVM RPC.
fn port_kind_hint(node_rpc: &str) -> Option<&'static str> {
    let port: u16 = node_rpc.rsplit_once(':').and_then(|(_, p)| p.parse().ok())?;
    match port {
        26110 | 26210 | 26510 | 26610 => Some(
            "that port is node gRPC; the validator needs wRPC Borsh (testnet-10: 27210, devnet: 27610). Pass --node-wrpc-borsh <host:borsh-port>.",
        ),
        28110 | 28210 | 28510 | 28610 => Some(
            "that port is node wRPC JSON; the validator needs wRPC Borsh (testnet-10: 27210). Pass --node-wrpc-borsh <host:borsh-port>.",
        ),
        8545 => Some("that port is the EVM JSON-RPC; the validator does not use it. Pass --node-wrpc-borsh <host:borsh-port>."),
        _ => None,
    }
}

/// Resolve the node wRPC Borsh endpoint (design §7.3): an explicit `--node-wrpc-borsh`
/// / `KASPA_PQ_NODE_RPC` wins; else the local endpoint registry the node wrote
/// (`~/.misaka/<network-id>/endpoints.json`, which carries the actual bound port even if
/// non-standard); else the network-id default loopback. So a bare `--network-id testnet-10`
/// finds a co-located node with no port typed. The result is still re-verified against the
/// node's own network-id after connecting (the existing post-connect guard).
fn resolve_node_rpc(network: &Option<String>, explicit: &Option<String>) -> String {
    if let Some(e) = explicit {
        return e.clone();
    }
    if let Some(net) = network
        && let Ok(nid) = NetworkId::from_str(net)
    {
        return misaka_endpoints::resolve(
            &nid,
            EndpointKind::NodeWrpcBorsh,
            None,
            misaka_endpoints::EndpointRegistry::load(net).as_ref(),
        );
    }
    "127.0.0.1:27210".to_string()
}

async fn connect(node_rpc: &str) -> Result<KaspaRpcClient, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| format!("failed to build wRPC client: {e}"))?;
    // ConnectStrategy::Retry keeps the wRPC client's reconnection loop alive, so a node restart
    // (or any transient WebSocket drop) is recovered AUTOMATICALLY: the validator resumes attesting
    // once the node is back, instead of getting wedged in "WebSocket is not connected; retrying"
    // forever (Fallback tears the reconnect loop down on the first failure). `block_async_connect`
    // still waits for the FIRST connection so the network-id guard + first attestation run against a
    // live node. This lets the utility survive node
    // restarts unattended — important on every network (a node bounce no longer silently stops
    // attestation, which would otherwise degrade DNS finality until a manual restart).
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Retry,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| {
        let mut msg = format!("failed to connect to node {url}: {e}");
        if let Some(hint) = port_kind_hint(node_rpc) {
            msg.push_str(&format!("\nhint: {hint}"));
        }
        msg
    })?;
    Ok(client)
}

#[derive(Debug, Clone)]
struct FundingCandidate {
    outpoint: TransactionOutpoint,
    entry: UtxoEntry,
}

fn push_top_funding(top: &mut Vec<FundingCandidate>, candidate: FundingCandidate, max_candidates: usize) {
    if max_candidates == 0 {
        return;
    }
    if top.len() < max_candidates {
        top.push(candidate);
        return;
    }
    let Some((min_idx, min)) = top.iter().enumerate().min_by_key(|(_, e)| e.entry.amount) else {
        return;
    };
    if candidate.entry.amount > min.entry.amount {
        top[min_idx] = candidate;
    }
}

/// Scan the funding address through the paged UTXO API and keep only the largest mature
/// candidates needed by a command. This avoids the legacy unbounded `getUtxosByAddresses`
/// response on miner-contaminated addresses with 100k+ coinbase fragments.
async fn top_mature_funding_paged(
    client: &KaspaRpcClient,
    funding_addr: &Address,
    virtual_daa: u64,
    coinbase_maturity: u64,
    max_candidates: usize,
    exclude: Option<TransactionOutpoint>,
) -> Result<(Vec<FundingCandidate>, usize), String> {
    const PAGE_LIMIT: u64 = 1000;
    if max_candidates == 0 {
        return Ok((Vec::new(), 0));
    }
    let mut cursor = String::new();
    let mut top = Vec::new();
    let mut mature_seen = 0usize;
    loop {
        let page = client
            .get_utxos_by_address_page(funding_addr.clone(), cursor, PAGE_LIMIT)
            .await
            .map_err(|e| format!("getUtxosByAddressPage failed (does the node run --utxoindex?): {e}"))?;
        let next_cursor = page.next_cursor;
        for e in page.entries {
            let outpoint = TransactionOutpoint::from(e.outpoint);
            if exclude.as_ref().map(|x| *x == outpoint).unwrap_or(false) {
                continue;
            }
            let entry = UtxoEntry::from(e.utxo_entry);
            if !is_spendable(entry.is_coinbase, entry.block_daa_score, virtual_daa, coinbase_maturity) {
                continue;
            }
            mature_seen += 1;
            push_top_funding(&mut top, FundingCandidate { outpoint, entry }, max_candidates);
        }
        if next_cursor.is_empty() {
            break;
        }
        cursor = next_cursor;
    }
    top.sort_by(|a, b| b.entry.amount.cmp(&a.entry.amount));
    Ok((top, mature_seen))
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
    // Accept both the NetworkType form ("testnet") and the NetworkId form
    // ("testnet-10") so the unified CLI's --network-id flows through keygen; the
    // testnet suffix only affects the P2P port, never the address prefix.
    let base = s.split('-').next().unwrap_or(s);
    match base.to_ascii_lowercase().as_str() {
        "mainnet" => Ok(Prefix::Mainnet),
        "testnet" => Ok(Prefix::Testnet),
        "devnet" => Ok(Prefix::Devnet),
        "simnet" => Ok(Prefix::Simnet),
        _ => Err(format!("unknown network '{s}' (expected mainnet/testnet/devnet/simnet, optionally with a -suffix)")),
    }
}

/// Parse a stake/amount string into u64 sompi (design §13.3). Accepts:
///   - a bare integer or `<n>sompi` — already sompi (whole numbers only);
///   - `<n>BILI` / `<n.m>BILI` — 1 BILI = 100_000_000 sompi, up to 8 decimals;
///     legacy `MSK` and `KAS` suffixes remain accepted.
/// Integer math throughout (no f64 precision loss); rejects junk and u64 overflow.
fn parse_amount_sompi(s: &str) -> Result<u64, String> {
    const SOMPI_PER_MSK: u64 = 100_000_000;
    let s = s.trim();
    if s.is_empty() {
        return Err("amount is empty".to_string());
    }
    let lower = s.to_ascii_lowercase();
    let (num, is_coins) = if let Some(n) = lower.strip_suffix("sompi") {
        (n.trim(), false)
    } else if let Some(n) = lower.strip_suffix("bili") {
        (n.trim(), true)
    } else if let Some(n) = lower.strip_suffix("msk") {
        (n.trim(), true)
    } else if let Some(n) = lower.strip_suffix("kas") {
        (n.trim(), true)
    } else {
        (lower.as_str(), false) // bare = sompi
    };
    if num.is_empty() {
        return Err(format!("missing number in amount '{s}'"));
    }
    if !is_coins {
        // sompi is indivisible — whole numbers only.
        return num.parse::<u64>().map_err(|_| format!("invalid sompi amount '{s}' (must be a whole number)"));
    }
    // BILI (and legacy aliases): fixed-point with up to 8 fractional digits, parsed as integers.
    let (int_part, frac_part) = num.split_once('.').unwrap_or((num, ""));
    if frac_part.len() > 8 {
        return Err(format!("too many decimals in '{s}' (max 8 for BILI)"));
    }
    if !int_part.chars().all(|c| c.is_ascii_digit()) || !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("invalid amount '{s}'"));
    }
    let int_v: u64 = if int_part.is_empty() { 0 } else { int_part.parse().map_err(|_| format!("invalid amount '{s}'"))? };
    // Right-pad the fraction to 8 digits → its value is directly in sompi.
    let frac_v: u64 =
        if frac_part.is_empty() { 0 } else { format!("{frac_part:0<8}").parse().map_err(|_| format!("invalid fraction in '{s}'"))? };
    int_v
        .checked_mul(SOMPI_PER_MSK)
        .and_then(|whole| whole.checked_add(frac_v))
        .ok_or_else(|| format!("amount '{s}' overflows u64 sompi"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn retired_dns_commands_are_unknown() {
        use clap::Parser;
        for command in ["run", "bond", "equivocate"] {
            assert!(super::Cli::try_parse_from(["kaspa-pq-validator", command]).is_err());
        }
    }
    use super::*;

    #[test]
    fn parse_prefix_known_and_unknown() {
        assert_eq!(parse_prefix("mainnet").unwrap(), Prefix::Mainnet);
        assert_eq!(parse_prefix("SIMNET").unwrap(), Prefix::Simnet);
        // NetworkId suffix form (the unified CLI's --network-id) resolves to the base prefix.
        assert_eq!(parse_prefix("testnet-10").unwrap(), Prefix::Testnet);
        assert_eq!(parse_prefix("testnet-11").unwrap(), Prefix::Testnet);
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
    fn parse_amount_sompi_units() {
        // Current ticker: exact smallest-unit conversion, case-insensitive, no float rounding.
        assert_eq!(parse_amount_sompi("10BILI").unwrap(), 1_000_000_000);
        assert_eq!(parse_amount_sompi(" 10.5 bili ").unwrap(), 1_050_000_000);
        assert_eq!(parse_amount_sompi("0.00000001BILI").unwrap(), 1);
        assert_eq!(parse_amount_sompi(".5BILI").unwrap(), 50_000_000);
        assert_eq!(parse_amount_sompi("184467440737.09551615BILI").unwrap(), u64::MAX);
        // bare + explicit sompi
        assert_eq!(parse_amount_sompi("1000000000").unwrap(), 1_000_000_000);
        assert_eq!(parse_amount_sompi("1000000000sompi").unwrap(), 1_000_000_000);
        assert_eq!(parse_amount_sompi("0").unwrap(), 0);
        // MSK / KAS (1 coin = 1e8 sompi), incl. fractional + case-insensitive + spaces
        assert_eq!(parse_amount_sompi("10MSK").unwrap(), 10 * 100_000_000);
        assert_eq!(parse_amount_sompi("10kas").unwrap(), 10 * 100_000_000);
        assert_eq!(parse_amount_sompi("10.5MSK").unwrap(), 1_050_000_000);
        assert_eq!(parse_amount_sompi("0.00000001MSK").unwrap(), 1); // 1 sompi
        assert_eq!(parse_amount_sompi(".5MSK").unwrap(), 50_000_000);
        assert_eq!(parse_amount_sompi(" 2 MSK ").unwrap(), 2 * 100_000_000);
    }

    #[test]
    fn port_kind_hint_flags_wrong_services() {
        assert!(port_kind_hint("127.0.0.1:26210").unwrap().contains("gRPC")); // testnet gRPC
        assert!(port_kind_hint("127.0.0.1:28210").unwrap().contains("wRPC JSON"));
        assert!(port_kind_hint("127.0.0.1:8545").unwrap().contains("EVM"));
        assert!(port_kind_hint("127.0.0.1:27210").is_none(), "borsh port has no hint");
        assert!(port_kind_hint("127.0.0.1:27610").is_none(), "devnet borsh port has no hint");
        assert!(port_kind_hint("garbage").is_none());
    }

    #[test]
    fn resolve_node_rpc_explicit_and_fallback() {
        // explicit --node-wrpc-borsh / env wins over the network
        assert_eq!(resolve_node_rpc(&Some("testnet-10".into()), &Some("1.2.3.4:9".into())), "1.2.3.4:9");
        // no network + no explicit → the loopback fallback default
        assert_eq!(resolve_node_rpc(&None, &None), "127.0.0.1:27210");
        // an unparseable network-id with no explicit → fallback (never panics)
        assert_eq!(resolve_node_rpc(&Some("bogus-net".into()), &None), "127.0.0.1:27210");
        // (the network-default and registry branches are covered by misaka_endpoints::resolve tests)
    }

    #[test]
    fn parse_amount_sompi_rejects_junk() {
        assert!(parse_amount_sompi("BILI").is_err());
        assert!(parse_amount_sompi("10.123456789BILI").is_err(), "more than 8 decimals");
        assert!(parse_amount_sompi("abcBILI").is_err());
        assert!(parse_amount_sompi("184467440737.09551616BILI").is_err(), "one sompi past u64::MAX");
        assert!(parse_amount_sompi("").is_err());
        assert!(parse_amount_sompi("MSK").is_err());
        assert!(parse_amount_sompi("10.5").is_err(), "fractional sompi is invalid");
        assert!(parse_amount_sompi("10.123456789MSK").is_err(), "more than 8 decimals");
        assert!(parse_amount_sompi("abcMSK").is_err());
        assert!(parse_amount_sompi("99999999999999999999MSK").is_err(), "overflow");
    }
}
