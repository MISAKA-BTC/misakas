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
    SignedEpochStore, VALIDATOR_SEED_LEN, ValidatorKey, is_spendable, load_validator_seed, parse_stake_bond_ref, select_funding,
};
use kaspa_rpc_core::{
    GetStakeBondRequest, GetValidatorAttestationTargetRequest, GetValidatorAttestationTargetResponse, RpcTransaction, api::rpc::RpcApi,
};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use rand::RngCore;
use std::collections::HashSet;
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
    /// kaspa-pq EVM Lane (§7.2): create an EVM_DEPOSIT_LOCK output funding an EVM address —
    /// the UTXO side of a bridge deposit. Claim it on a mining node afterwards via
    /// submitEvmDepositClaim(txid, 0).
    DepositLock(DepositLockArgs),
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
    /// canonical-ready epoch, so this poll period MUST be ≤ an epoch's wall-clock duration for a
    /// single validator to cover EVERY epoch and reach the DNS stake-depth threshold. ALL kaspa-pq
    /// networks (mainnet/testnet/devnet/simnet) run at 10 BPS (`BlockrateParams::new::<10>()`,
    /// target_time_per_block = 100 ms) with `attestation_epoch_length_blue_score = 100`, so an
    /// epoch is ≈ 10 s — hence the default 3 s (≈3 polls/epoch, keeps a single validator caught up
    /// on every network). Revisiting the same epoch within a run is deduped (no re-sign / no
    /// rebroadcast), so a small value only adds cheap local-node RPC polls; raise it only if you
    /// deliberately throttle the chain to a slower block rate.
    #[arg(long, default_value_t = 3, env = "KASPA_PQ_ATTEST_POLL_SECS")]
    attest_poll_secs: u64,

    /// Fee in sompi for each attestation-shard transaction. Default: a mass-based estimate from the
    /// network's mass params (the shard carries a 4627-byte ML-DSA-87 signature, so the flat floor
    /// is far below the mempool minimum — ≈ 232 600 sompi on devnet). Pass an explicit value to
    /// override (e.g. bump under congestion); like `bond`/`unbond`, omit it to auto-size.
    #[arg(long, env = "KASPA_PQ_ATTEST_FEE")]
    fee: Option<u64>,

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

#[derive(Parser, Debug)]
struct DepositLockArgs {
    /// Local node wRPC (borsh) endpoint, host:port. The node must run --utxoindex.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,
    /// ML-DSA key (32-byte seed, hex) whose funding address pays the deposit. Its own
    /// funding P2PKH becomes the lock's refund script (reclaimable after the timeout).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,
    /// The EVM address to credit, 20-byte hex (optional 0x prefix).
    #[arg(long)]
    evm_address: String,
    /// Deposit amount in sompi (locked into the EVM_DEPOSIT_LOCK output-0).
    #[arg(long)]
    amount: u64,
    /// Claim-inclusion tip (sompi, ≤ amount) paid to the accepting block's EVM coinbase —
    /// the §9.2 incentive for a producer to include the claim.
    #[arg(long, default_value_t = 0)]
    claim_tip: u64,
    /// Refund timeout as a DAA-score DELTA from the current sink (the lock is claimable
    /// strictly before sink_daa + delta; refundable to the funding key after).
    #[arg(long, default_value_t = 1_000_000)]
    timeout_daa_delta: u64,
    /// Fee in sompi. Default: a mass-based estimate (each ML-DSA input is ~7 KB).
    #[arg(long)]
    fee: Option<u64>,
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
        Command::DepositLock(args) => {
            kaspa_core::log::init_logger(None, "info");
            deposit_lock(args).await
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
    info!("[{VALIDATOR}] staking {} sompi as validator_id={} (funding {})", args.amount, key.validator_id, funding_addr);

    // Aggregate enough MATURE funding UTXOs to cover amount + fee. Mining pays the funding address
    // as many ~subsidy-sized coinbase fragments, so a single UTXO rarely covers a bond; sum the
    // largest mature ones (`build_funded_stake_bond_tx_multi`). A coinbase UTXO is unspendable
    // until `coinbase_maturity` blocks pass (consensus rule); a miner still paying this address
    // mints a fresh immature coinbase every block, so filter by maturity (else an immature pick
    // gets the bond tx rejected "spends an immature UTXO"). Mass-based fee unless overridden — the
    // StakeBond payload carries the 2592-byte pubkey and each ML-DSA-87 input is ~7 KB, so the fee
    // grows with the input count and is re-estimated as UTXOs are added.
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    let utxos = client
        .get_utxos_by_addresses(vec![funding_addr.clone()])
        .await
        .map_err(|e| format!("getUtxosByAddresses failed (does the node run --utxoindex?): {e}"))?;
    let mut mature: Vec<_> = utxos
        .into_iter()
        .filter(|e| is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, virtual_daa, coinbase_maturity))
        .collect();
    // Largest-first greedy selection. Cap the input count so the bond tx stays within the block
    // mass limit (each ML-DSA-87 input is ~7 KB); 20 comfortably fits a reasonable testnet bond.
    mature.sort_by(|a, b| b.utxo_entry.amount.cmp(&a.utxo_entry.amount));
    const MAX_BOND_INPUTS: usize = 20;
    let mut selected = Vec::new();
    let mut sum: u64 = 0;
    let mut fee = match args.fee {
        Some(f) => f,
        None => key.estimate_bond_fee_for_inputs(&mass_calc, prefix, 1),
    };
    for e in mature.into_iter() {
        if selected.len() >= MAX_BOND_INPUTS {
            break;
        }
        sum = sum.saturating_add(e.utxo_entry.amount);
        selected.push(e);
        if args.fee.is_none() {
            fee = key.estimate_bond_fee_for_inputs(&mass_calc, prefix, selected.len());
        }
        if sum >= args.amount.saturating_add(fee) {
            break;
        }
    }
    let needed = args.amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
    if selected.is_empty() || sum < needed {
        return Err(format!(
            "not enough MATURE funding at {funding_addr}: have {sum} sompi across {} mature UTXO(s) (cap {MAX_BOND_INPUTS}), \
             need {needed} sompi (amount {} + fee {fee}). Mine more to this address and wait for coinbase maturity \
             ({coinbase_maturity} blocks), or lower --amount.",
            selected.len(),
            args.amount
        ));
    }
    info!(
        "[{VALIDATOR}] funding bond from {} mature UTXO(s) totalling {sum} sompi (fee {fee} sompi{})",
        selected.len(),
        if args.fee.is_some() { "" } else { ", mass-based" }
    );
    let fundings: Vec<(TransactionOutpoint, UtxoEntry)> =
        selected.into_iter().map(|e| (e.outpoint.into(), e.utxo_entry.into())).collect();

    let tx = key.build_funded_stake_bond_tx_multi(
        args.amount,
        args.activation_daa_score,
        args.unbonding_period_blocks,
        key.reward_spk_payload(),
        &fundings,
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

/// kaspa-pq EVM Lane (§7.2): create an EVM_DEPOSIT_LOCK output — the UTXO side of a bridge
/// deposit. Mirrors `bond`'s mature-UTXO aggregation; output-0 is the lock binding the EVM
/// credit address / refund timeout / claim tip, refund script = this key's own funding P2PKH.
async fn deposit_lock(args: DepositLockArgs) -> Result<(), String> {
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
    // Audit F4: refuse to create a deposit-lock on a network where the EVM lane is inert
    // (mainnet/simnet, or before activation). The claim path can never run there, so such a lock
    // could only be REFUNDED after its timeout (and a near-u64::MAX timeout would strand the funds
    // effectively forever). This is a CLI-side guard only — non-consensus, and the refund path
    // itself stays open so any lock that does exist remains recoverable.
    if !params.is_evm_active(server.virtual_daa_score) {
        return Err(format!(
            "EVM lane is not active on '{node_network}' (evm_activation_daa_score not reached; mainnet/simnet are inert) — \
             a deposit-lock here could only be refunded after the timeout, never claimed. Refusing to create it."
        ));
    }
    let mass_calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );

    // 20-byte EVM address (optional 0x).
    let evm_hex = args.evm_address.strip_prefix("0x").or_else(|| args.evm_address.strip_prefix("0X")).unwrap_or(&args.evm_address);
    if evm_hex.len() != 40 {
        return Err(format!("--evm-address must be 40 hex chars (20 bytes), got {}", evm_hex.len()));
    }
    let mut evm_address = [0u8; 20];
    faster_hex::hex_decode(evm_hex.as_bytes(), &mut evm_address).map_err(|e| format!("malformed --evm-address: {e}"))?;

    let timeout_daa_score = server.virtual_daa_score.saturating_add(args.timeout_daa_delta);
    info!(
        "[{VALIDATOR}] depositing {} sompi to EVM 0x{evm_hex} (tip {}, refund timeout daa {timeout_daa_score}, funding {funding_addr})",
        args.amount, args.claim_tip
    );

    // Same mature-UTXO aggregation as `bond`.
    let coinbase_maturity = params.coinbase_maturity();
    let virtual_daa = server.virtual_daa_score;
    let utxos = client
        .get_utxos_by_addresses(vec![funding_addr.clone()])
        .await
        .map_err(|e| format!("getUtxosByAddresses failed (does the node run --utxoindex?): {e}"))?;
    let mut mature: Vec<_> = utxos
        .into_iter()
        .filter(|e| is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, virtual_daa, coinbase_maturity))
        .collect();
    mature.sort_by(|a, b| b.utxo_entry.amount.cmp(&a.utxo_entry.amount));
    const MAX_DEPOSIT_INPUTS: usize = 20;
    let mut selected = Vec::new();
    let mut sum: u64 = 0;
    let mut fee = match args.fee {
        Some(f) => f,
        None => key.estimate_deposit_lock_fee_for_inputs(&mass_calc, prefix, 1),
    };
    for e in mature.into_iter() {
        if selected.len() >= MAX_DEPOSIT_INPUTS {
            break;
        }
        sum = sum.saturating_add(e.utxo_entry.amount);
        selected.push(e);
        if args.fee.is_none() {
            fee = key.estimate_deposit_lock_fee_for_inputs(&mass_calc, prefix, selected.len());
        }
        if sum >= args.amount.saturating_add(fee) {
            break;
        }
    }
    let needed = args.amount.checked_add(fee).ok_or_else(|| "amount + fee overflows u64".to_string())?;
    if selected.is_empty() || sum < needed {
        return Err(format!(
            "not enough MATURE funding at {funding_addr}: have {sum} sompi across {} mature UTXO(s) (cap {MAX_DEPOSIT_INPUTS}), \
             need {needed} sompi (amount {} + fee {fee}).",
            selected.len(),
            args.amount
        ));
    }
    info!(
        "[{VALIDATOR}] funding deposit from {} mature UTXO(s) totalling {sum} sompi (fee {fee} sompi{})",
        selected.len(),
        if args.fee.is_some() { "" } else { ", mass-based" }
    );
    let fundings: Vec<(TransactionOutpoint, UtxoEntry)> =
        selected.into_iter().map(|e| (e.outpoint.into(), e.utxo_entry.into())).collect();

    let tx = key.build_funded_deposit_lock_tx_multi(args.amount, evm_address, timeout_daa_score, args.claim_tip, &fundings, fee)?;
    let txid =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| format!("submitTransaction failed: {e}"))?;
    info!("[{VALIDATOR}] submitted deposit-lock tx (txid={txid})");
    println!("deposit_lock_outpoint: {txid}:0");
    println!("(once accepted, claim on a MINING node: submitEvmDepositClaim {txid} 0 — the claim then executes in an accepting chain block and credits the EVM address)");
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
    // ConnectStrategy::Retry keeps the wRPC client's reconnection loop alive, so a node restart
    // (or any transient WebSocket drop) is recovered AUTOMATICALLY: the validator resumes attesting
    // once the node is back, instead of getting wedged in "WebSocket is not connected; retrying"
    // forever (Fallback tears the reconnect loop down on the first failure). `block_async_connect`
    // still waits for the FIRST connection so the network-id guard + first attestation run against a
    // live node. Combined with run_loop's per-round retry, this makes the validator survive node
    // restarts unattended — important on every network (a node bounce no longer silently stops
    // attestation, which would otherwise degrade DNS finality until a manual restart).
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Retry,
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
            "[{VALIDATOR}] attesting as validator_id={} (funding {}, fee {} sompi{})",
            a.key.validator_id,
            a.key.funding_address(prefix),
            a.attestation_fee,
            if args.fee.is_some() { "" } else { ", mass-based" }
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
    /// Attestation-shard fee (sompi), fixed once at load: the explicit `--fee` if given, else a
    /// mass-based estimate from the network's mass params (the shard tx shape is fixed, so the fee
    /// is constant across epochs). Either way it is far above the flat floor, which is below the
    /// kaspa-pq mempool minimum for this payload-heavy tx.
    attestation_fee: u64,
    /// The last epoch this PROCESS has already attested (submitted a shard for). Lets a short
    /// `--attest-poll-secs` revisit the same canonical-ready epoch cheaply without re-signing or
    /// rebroadcasting (which would burn a funding UTXO each poll). Reset on restart, so the
    /// persistent `SignedEpochStore` still drives a single crash-recovery rebroadcast.
    last_attested_epoch: Option<u64>,
    /// Head of the local funding chain: the change output (index 0, change back to self) of the
    /// most recently submitted attestation tx. The node's utxoindex keeps listing a just-spent
    /// funding UTXO as available until our tx is mined, so re-querying it each epoch re-selects an
    /// outpoint our own in-flight tx already spent → "output … already spent … in the mempool"
    /// rejection. Spending this change directly chains one funded hop per epoch across the
    /// unconfirmed window instead. In-memory only (reset on restart, which simply reselects a
    /// confirmed UTXO and starts a fresh chain).
    pending_change: Option<(TransactionOutpoint, UtxoEntry)>,
    /// Funding outpoints we have already spent in submitted (not-yet-mined) txs, so the node-query
    /// fallback never re-selects one. Pruned each tick to those the node still lists (mined-spent
    /// ones drop out), so it self-heals and stays tiny (≈ the few epochs still in the mempool).
    inflight_spent: HashSet<TransactionOutpoint>,
    /// kaspa-pq DNS-v3 hardening (Fix B): the epoch whose attestation produced the current
    /// `pending_change` chain head. `None` when there is no in-flight chain. Used to count
    /// distinct epochs the head has gone unconfirmed.
    chain_head_epoch: Option<u64>,
    /// kaspa-pq DNS-v3 hardening (Fix B): consecutive served epochs the funding-chain head has
    /// failed to confirm. Reset to 0 whenever the head confirms (node-set resync clears
    /// `pending_change`) or we abandon the chain and re-fund from a confirmed node UTXO. When it
    /// reaches `N_STALL_EPOCHS` the chain is abandoned, breaking a stuck cascade that would
    /// otherwise never self-recover (the live-testnet dnsConfirmed-stall root cause).
    stalled_epochs: u64,
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
        // Mass-based fee unless overridden (mirrors `bond`/`unbond`): an explicit `--fee` wins, else
        // size it from the network mass params (≈ 290 000 sompi for the shard's 4627-byte signature).
        let attestation_fee = args.fee.unwrap_or_else(|| key.estimate_attestation_fee(mass_calc, prefix));
        Ok(Some(Self {
            key,
            bond_outpoint,
            signed_store,
            prefix,
            coinbase_maturity,
            attestation_fee,
            last_attested_epoch: None,
            pending_change: None,
            inflight_spent: HashSet::new(),
            chain_head_epoch: None,
            stalled_epochs: 0,
        }))
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

        // The node's utxoindex reflects the ACCEPTED (mined) UTXO set, not the mempool: it keeps
        // listing a funding UTXO our own still-unconfirmed attestation tx has already spent, so re-
        // selecting it is rejected ("output … already spent … in the mempool"). Drive a local
        // funding chain instead — spend the change output of the previous tx, which the mempool
        // accepts as a chained spend of an unconfirmed parent. One attestation per epoch ⇒ one
        // funded hop per epoch across the unconfirmed window.
        let node_utxos: Vec<(TransactionOutpoint, UtxoEntry)> =
            utxos.into_iter().map(|e| (TransactionOutpoint::from(e.outpoint), UtxoEntry::from(e.utxo_entry))).collect();
        let node_outpoints: HashSet<TransactionOutpoint> = node_utxos.iter().map(|(op, _)| *op).collect();
        // Forget in-flight exclusions the node no longer lists (those txs were mined ⇒ no risk of
        // re-selecting them): self-heals and keeps the set tiny (≈ the few epochs still in mempool).
        self.inflight_spent.retain(|op| node_outpoints.contains(op));
        // If our chain head has been mined (now appears in the node set), resync to the node view.
        if let Some((head, _)) = &self.pending_change {
            if node_outpoints.contains(head) {
                self.pending_change = None;
            }
        }
        // kaspa-pq DNS-v3 hardening (Fix B — stuck-chain recovery): if the head did NOT just confirm
        // (pending_change still set), count distinct served epochs it has stalled. After
        // N_STALL_EPOCHS, abandon the unconfirmed chain so select_funding falls back to a CONFIRMED
        // node UTXO — breaking a cascade that otherwise never self-recovers (before this, only a
        // process restart cleared it; that was the live-testnet dnsConfirmed-stall root cause).
        // Catches every stall mode: §B.4 ineligibility, a reorg-dropped parent, mempool eviction, a
        // too-low fee under congestion. The Fix-A start-gate prevents the §B.4 mode up front; this
        // is the belt-and-suspenders that recovers from the rest.
        const N_STALL_EPOCHS: u64 = 3;
        if self.pending_change.is_some() {
            // attest() runs at most once per distinct epoch (the run loop short-circuits repeats via
            // last_attested_epoch), so a changed target.epoch means another whole epoch elapsed
            // without the head confirming.
            if self.chain_head_epoch != Some(target.epoch) {
                self.stalled_epochs = self.stalled_epochs.saturating_add(1);
            }
            if self.stalled_epochs >= N_STALL_EPOCHS {
                warn!(
                    "[{VALIDATOR}] funding-chain head unmined for {} epochs (now epoch {}); abandoning the unconfirmed chain and re-funding from a confirmed UTXO",
                    self.stalled_epochs, target.epoch
                );
                // Drop ONLY the chain head. Do NOT clear inflight_spent: the stalled tx still holds
                // its funding outpoint spent-in-mempool, but the node's utxoindex (accepted set, no
                // mempool subtraction — see the comment above) keeps LISTING that outpoint, so
                // re-picking it would just RejectDoubleSpendInMempool and stall again. Keeping the
                // exclusion forces select_funding onto a DIFFERENT mature node UTXO = real recovery;
                // the retain above self-heals inflight_spent once the stalled tx mines or expires.
                self.pending_change = None;
                self.stalled_epochs = 0;
                self.chain_head_epoch = None;
            }
        } else {
            // Head confirmed (resync cleared it) or no chain yet → healthy.
            self.stalled_epochs = 0;
        }
        let (funding_outpoint, funding_entry) =
            select_funding(&self.pending_change, &self.inflight_spent, node_utxos, fee, virtual_daa, self.coinbase_maturity)?;

        let tx = self.key.build_funded_shard_tx(&shard, funding_outpoint, &funding_entry, fee)?;

        // Persist the signing record BEFORE broadcasting, so a crash post-submit cannot lose
        // the record and let a restart sign a different target for this epoch.
        if outcome == SignedEpochCheckOutcome::Allow {
            self.signed_store.record_and_flush(record)?;
        }

        match client.submit_transaction(RpcTransaction::from(&tx), false).await {
            Ok(txid) => {
                info!("[{VALIDATOR}] submitted attestation shard for epoch {} (txid={txid})", target.epoch);
                // Advance the funding chain: this tx's change output (index 0, back to self) funds the
                // next epoch. The tx id excludes signature scripts, so it is stable post-sign and
                // matches the id the node assigns.
                self.inflight_spent.insert(funding_outpoint);
                let change =
                    UtxoEntry::new(funding_entry.amount - fee, funding_entry.script_public_key.clone(), virtual_daa, false);
                self.pending_change = Some((TransactionOutpoint::new(tx.id(), 0), change));
                // kaspa-pq DNS-v3 hardening (Fix B): record which epoch produced this chain head so
                // the stall counter advances once per unconfirmed epoch.
                self.chain_head_epoch = Some(target.epoch);
                Ok(())
            }
            Err(e) => {
                // Submit failed ⇒ no new change output exists. Drop the chain head so the next tick
                // reselects from the node; the in-flight set still excludes UTXOs our earlier
                // (accepted) txs spent, so the fallback won't re-pick a mempool-spent outpoint.
                self.pending_change = None;
                Err(format!("submitTransaction failed: {e}"))
            }
        }
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
                    // kaspa-pq DNS-v3 hardening (Fix A — anchor-deep start-gate): never attest an
                    // epoch whose canonical lagged anchor predates the bond's activation. The
                    // consensus §B.4 rule (attestation_reward_eligibility → active_bond_at(..,
                    // target_daa_score)) makes ANY block that includes such a shard INVALID, so the
                    // shard would submit-OK but never be mined. On a young chain (e.g. right after a
                    // re-genesis) the lagged anchor can sit below the bond's activation_daa_score for
                    // the first epochs; attesting then would stall the whole funding chain (see Fix B).
                    // Gate until the served target is at/after activation — the exact §B.4 condition.
                    Ok(t) if t.available && t.target_daa_score < bond_resp.activation_daa_score => {
                        info!(
                            "[{VALIDATOR}] status=ActiveBelowActivation epoch={} target_daa={} < activation_daa={} (gating until bond is anchor-deep)",
                            t.epoch, t.target_daa_score, bond_resp.activation_daa_score
                        );
                    }
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
    fn decode_message_roundtrip_and_reject() {
        let bytes = [0xABu8; 32];
        let mut hex = [0u8; 64];
        faster_hex::hex_encode(&bytes, &mut hex).unwrap();
        let decoded = decode_message(std::str::from_utf8(&hex).unwrap()).unwrap();
        assert_eq!(decoded, bytes);
        assert!(decode_message("zz").is_err());
    }
}
