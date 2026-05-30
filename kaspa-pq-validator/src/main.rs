//! kaspa-pq-validator — the ADR-0011 single-host validator sidecar.
//!
//! A standalone process that connects to a co-located `kaspad` over a 127.0.0.1 wRPC
//! (borsh) endpoint and, once its stake bond is active, attests to the selected-chain
//! anchor each epoch: it fetches the ready-to-sign target over wRPC, signs it with its
//! ML-DSA-65 validator key (under the equivocation-safety guard), funds a
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
use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_core::{info, warn};
use kaspa_pq_validator_core::{
    ATTESTATION_TX_FEE_FLOOR_SOMPI, SignedEpochStore, VALIDATOR_SEED_LEN, ValidatorKey, load_validator_seed, parse_stake_bond_ref,
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
    /// Generate a new ML-DSA-65 validator key and print its identity + funding address.
    Keygen(KeygenArgs),
    /// One-shot: query the node + bond status and print it.
    Status(StatusArgs),
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

    /// Path to the ML-DSA-65 validator signing key (32-byte seed, hex). Required to attest.
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
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("[{VALIDATOR}] error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Generate a fresh ML-DSA-65 validator key, write the seed to `--out`, and print the
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
    info!("[{VALIDATOR}] connected: network={node_network} synced={} version={}", server.is_synced, server.server_version);

    // Load the signing identity if fully configured (key + bond + state DB); else observe.
    let attestor = Attestor::load(&args, prefix)?;
    match &attestor {
        Some(a) => info!("[{VALIDATOR}] attesting as validator_id={} (funding {})", a.key.validator_id, a.key.funding_address(prefix)),
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

/// The ML-DSA-65 signing identity + equivocation guard, present only when fully
/// configured. Shares its primitives with the in-process service via
/// `kaspa-pq-validator-core`.
struct Attestor {
    key: ValidatorKey,
    bond_outpoint: TransactionOutpoint,
    signed_store: SignedEpochStore,
    prefix: Prefix,
}

impl Attestor {
    /// Load the signing identity iff `--validator-key`, `--stake-bond` and
    /// `--signed-epoch-db` are all provided. The state file is rejected if it belongs to a
    /// different validator/bond (cross-key equivocation guard).
    fn load(args: &RunArgs, prefix: Prefix) -> Result<Option<Self>, String> {
        let (Some(key_path), Some(bond_ref), Some(db)) = (&args.validator_key, &args.stake_bond, &args.signed_epoch_db) else {
            return Ok(None);
        };
        let key = ValidatorKey::from_seed(load_validator_seed(key_path)?);
        let bond_outpoint = parse_stake_bond_ref(bond_ref)?;
        let signed_store = SignedEpochStore::load_or_empty(db.into(), key.validator_id, bond_outpoint)?;
        Ok(Some(Self { key, bond_outpoint, signed_store, prefix }))
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
        let fee = ATTESTATION_TX_FEE_FLOOR_SOMPI;
        let funding_addr = self.key.funding_address(self.prefix);
        let utxos = client
            .get_utxos_by_addresses(vec![funding_addr])
            .await
            .map_err(|e| format!("getUtxosByAddresses failed (does the node run --utxoindex?): {e}"))?;
        let funding = utxos
            .into_iter()
            .find(|e| e.utxo_entry.amount > fee)
            .ok_or_else(|| format!("no funding UTXO > {fee} sompi at the validator funding address; send funds there"))?;
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
                        Some(a) => {
                            if let Err(e) = a.attest(client, &t, args.dry_run).await {
                                warn!("[{VALIDATOR}] attest failed for epoch {}: {e}", t.epoch);
                            }
                        }
                        None => info!(
                            "[{VALIDATOR}] status=ActiveEligible epoch={} target={} (observe-only; not signing)",
                            t.epoch, t.target_hash
                        ),
                    },
                    Ok(_) => info!("[{VALIDATOR}] status=ActiveIdle (no attestation target available this tick)"),
                    Err(e) => warn!("[{VALIDATOR}] getValidatorAttestationTarget failed: {e}"),
                }
                sleep_secs(30).await;
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
