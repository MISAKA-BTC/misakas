//! kaspa-pq-validator — the ADR-0011 single-host validator sidecar.
//!
//! A standalone process that connects to a co-located `kaspad` over a 127.0.0.1 wRPC
//! (borsh) endpoint and, once its stake bond is active, attests to the selected-chain
//! anchor each epoch. This is the **skeleton** slice (ADR-0011 slot 10.6′): it wires
//! the wRPC client, the network/sync start-up guards, and the ADR-0011 status state
//! machine (`NodeNotSynced → BondNotFound → BondPending → Active → …`) with `--dry-run`
//! support. It does **not** yet load the validator key, sign, or submit — that lands in
//! the next slice. The recommended deployment runs this beside `kaspad` under systemd
//! (see ADR-0011 §"Systemd reference units").

use clap::Parser;
use kaspa_core::{info, warn};
use kaspa_rpc_core::{GetStakeBondRequest, GetValidatorAttestationTargetRequest, api::rpc::RpcApi};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use std::process::ExitCode;
use std::time::Duration;

const VALIDATOR: &str = "kaspa-pq-validator";

/// Kaspa-PQ validator sidecar (ADR-0011). Attests to selected-chain anchors when its
/// stake bond is active; connects to a local node over wRPC.
#[derive(Parser, Debug)]
#[command(name = "kaspa-pq-validator", version, about)]
struct Args {
    /// Local node wRPC (borsh) endpoint, host:port. Bind the node's RPC to 127.0.0.1 only.
    #[arg(long, default_value = "127.0.0.1:17110", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: String,

    /// Stake-bond outpoint backing this validator, "txid_hex:index". Without it the
    /// sidecar only observes (no attestation eligibility).
    #[arg(long, env = "KASPA_PQ_STAKE_BOND")]
    stake_bond: Option<String>,

    /// Path to the ML-DSA-65 validator signing key. Skeleton slice: not yet loaded —
    /// signing lands in the next slice.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: Option<String>,

    /// Compute eligibility + the attestation target and log them, but never sign or
    /// submit. New operators should run this first to verify their bond.
    #[arg(long, env = "KASPA_PQ_DRY_RUN")]
    dry_run: bool,

    /// Expected node network id (e.g. "kaspa-pq-mainnet"); refuse to start on mismatch
    /// (ADR-0011 §"Same network"). When omitted, the node's network is logged but trusted.
    #[arg(long, env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Logging level {off, error, warn, info, debug, trace}.
    #[arg(long, default_value = "info", env = "KASPA_PQ_LOG_LEVEL")]
    log_level: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    kaspa_core::log::init_logger(None, &args.log_level);

    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            warn!("[{VALIDATOR}] exiting: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), String> {
    let url = format!("ws://{}", args.node_rpc);
    info!("[{VALIDATOR}] connecting to local node at {url} (dry_run={})", args.dry_run);
    if args.validator_key.is_some() {
        info!("[{VALIDATOR}] a validator key was provided but this skeleton slice does not sign yet");
    }

    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| format!("failed to build wRPC client: {e}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(5_000)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| format!("failed to connect to node {url}: {e}"))?;

    // Network-id guard (ADR-0011 §"Same network"): never attest against the wrong net.
    let server = client.get_server_info().await.map_err(|e| format!("getServerInfo failed: {e}"))?;
    let node_network = server.network_id.to_string();
    match args.network.as_deref() {
        Some(expected) if node_network != expected => {
            return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
        }
        _ => {}
    }
    info!("[{VALIDATOR}] connected: network={node_network} synced={} version={}", server.is_synced, server.server_version);

    // ADR-0011 §"Auto-startup ordering": tolerate every "not yet" state and loop until
    // shutdown. Ctrl-C / SIGINT exits cleanly.
    let result = tokio::select! {
        r = run_loop(&client, &args) => r,
        _ = tokio::signal::ctrl_c() => {
            info!("[{VALIDATOR}] shutdown signal received");
            Ok(())
        }
    };
    let _ = client.disconnect().await;
    result
}

/// The ADR-0011 validator runtime loop. Skeleton: computes the status and (when active)
/// fetches the ready-to-sign target, but does not sign/submit. Returns `Err` only on the
/// fatal `Slashed` state; every other state sleeps and retries.
async fn run_loop(client: &KaspaRpcClient, args: &Args) -> Result<(), String> {
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
                // target; signing + submission land in the next slice.
                match client
                    .get_validator_attestation_target(GetValidatorAttestationTargetRequest { bond_outpoint: bond.to_owned() })
                    .await
                {
                    Ok(t) if t.available => {
                        if args.dry_run {
                            info!(
                                "[{VALIDATOR}] status=ActiveEligible DRY-RUN epoch={} target={} message={} (not signing)",
                                t.epoch, t.target_hash, t.message
                            );
                        } else {
                            info!(
                                "[{VALIDATOR}] status=ActiveEligible epoch={} target={} (signing lands in the next slice; not yet submitting)",
                                t.epoch, t.target_hash
                            );
                        }
                    }
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

async fn sleep_secs(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}
