//! Node operations. Tier A: `node doctor` — a one-shot health check that
//! surfaces the failure modes operators keep hitting (gRPC vs wRPC port
//! confusion, wRPC not started, utxoindex off, node/network mismatch, sync
//! phase ambiguity). Read-only: probes TCP ports + `getServerInfo` + the EVM
//! RPC; never mutates state.

use std::net::{TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::time::Duration;

use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    KaspaRpcClient, WrpcEncoding,
};

use crate::{exit, CliError, CliResult, OutputFormat};

/// Shared CLI context (the resolved global flags).
pub struct Ctx {
    pub output: OutputFormat,
    pub network: String,
    pub rpc: Option<String>,
    pub evm_rpc: String,
    pub timeout_secs: u64,
    pub quiet: bool,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Health {
    Ok,
    Info,
    Warn,
    Fail,
}
impl Health {
    fn tag(self) -> &'static str {
        match self {
            Health::Ok => "OK",
            Health::Info => "INFO",
            Health::Warn => "WARN",
            Health::Fail => "FAIL",
        }
    }
}

struct Row {
    label: String,
    value: String,
    health: Health,
}

fn probe_port(host: &str, port: u16, timeout: Duration) -> bool {
    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .map(|sa| TcpStream::connect_timeout(&sa, timeout).is_ok())
        .unwrap_or(false)
}

async fn try_connect(url: &str, timeout: Duration) -> Result<KaspaRpcClient, String> {
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(url), None, None, None).map_err(|e| e.to_string())?;
    // One-shot (Fallback): doctor does not keep a reconnect loop alive.
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(timeout),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client.connect(Some(options)).await.map_err(|e| e.to_string())?;
    Ok(client)
}

pub async fn doctor(ctx: &Ctx) -> CliResult {
    let net_id = NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let nt: NetworkType = net_id.network_type();
    let timeout = Duration::from_secs(ctx.timeout_secs.clamp(2, 10));

    // Resolve the borsh endpoint, then derive the sibling ports RELATIVE to it.
    // Across all networks gRPC = borsh-1000, json = borsh+1000, P2P = gRPC+1, so a
    // deployment that rebinds the whole base (e.g. borsh 27610 => gRPC 26610 /
    // json 28610 / P2P 26611) is probed correctly instead of against the code
    // defaults. Without --rpc, fall back to this network's default borsh port.
    let (probe_host, borsh_port) = match &ctx.rpc {
        Some(hp) => {
            let (h, p) = hp.rsplit_once(':').ok_or_else(|| CliError::new(exit::GENERIC, format!("--rpc must be host:port (got {hp})")))?;
            (h.to_string(), p.parse::<u16>().map_err(|_| CliError::new(exit::GENERIC, format!("bad --rpc port in {hp}")))?)
        }
        None => ("127.0.0.1".to_string(), nt.default_borsh_rpc_port()),
    };
    let grpc_port = borsh_port.saturating_sub(1000);
    let json_port = borsh_port.saturating_add(1000);
    let p2p_port = grpc_port + 1;
    let borsh_hostport = format!("{probe_host}:{borsh_port}");

    let mut rows: Vec<Row> = Vec::new();
    // The verdict/exit code is driven ONLY by authoritative checks (wRPC reach +
    // getServerInfo + network match + sync). The raw TCP port probes are
    // informational: a deployment can rebind ports independently (e.g. .213 puts
    // RPC on 266xx but P2P on 26211), so a "port not listening" guess must not
    // fail the verdict.
    let mut exit_code = exit::SUCCESS;

    rows.push(Row { label: "Network (expected)".into(), value: ctx.network.clone(), health: Health::Ok });

    // --- wRPC borsh: connect + getServerInfo (the authoritative view) ---
    let mut synced_phase = "unknown".to_string();
    let mut virtual_daa: Option<u64> = None;
    match try_connect(&format!("ws://{borsh_hostport}"), timeout).await {
        Ok(client) => match client.get_server_info().await {
            Ok(info) => {
                rows.push(Row { label: format!("wRPC Borsh {borsh_hostport}"), value: "listening".into(), health: Health::Ok });
                rows.push(Row { label: "kaspad version".into(), value: info.server_version.clone(), health: Health::Ok });

                let node_net = info.network_id.to_string();
                let net_ok = node_net == ctx.network;
                if !net_ok {
                    exit_code = exit::NETWORK_MISMATCH;
                }
                rows.push(Row {
                    label: "Node network".into(),
                    value: if net_ok { node_net } else { format!("{node_net}  (!= expected {})", ctx.network) },
                    health: if net_ok { Health::Ok } else { Health::Fail },
                });

                rows.push(Row {
                    label: "UTXO index".into(),
                    value: if info.has_utxo_index { "enabled".into() } else { "DISABLED (wallet/validator need it)".into() },
                    health: if info.has_utxo_index { Health::Ok } else { Health::Warn },
                });
                rows.push(Row {
                    label: "Synced".into(),
                    value: if info.is_synced { "true".into() } else { "false (IBD in progress)".into() },
                    health: if info.is_synced { Health::Ok } else { Health::Warn },
                });
                if !info.is_synced && exit_code == exit::SUCCESS {
                    exit_code = exit::NODE_NOT_SYNCED;
                }
                synced_phase = if info.is_synced { "synced".into() } else { "syncing".into() };
                virtual_daa = Some(info.virtual_daa_score);
                let _ = client.disconnect().await;
            }
            Err(e) => {
                exit_code = exit::CONNECTION;
                rows.push(Row { label: format!("wRPC Borsh {borsh_hostport}"), value: format!("getServerInfo failed: {e}"), health: Health::Fail });
            }
        },
        Err(e) => {
            exit_code = exit::CONNECTION;
            rows.push(Row {
                label: format!("wRPC Borsh {borsh_hostport}"),
                value: format!("UNREACHABLE ({e}) — is the node up with --rpclisten-borsh?"),
                health: Health::Fail,
            });
        }
    }

    // --- raw TCP port probes (informational; do not affect the verdict) ---
    for (label, port) in [
        (format!("gRPC {grpc_port}"), grpc_port),
        (format!("wRPC JSON {json_port}"), json_port),
        // P2P is derived (gRPC+1); deployments may rebind it independently.
        (format!("P2P {p2p_port} (derived)"), p2p_port),
    ] {
        let up = probe_port(&probe_host, port, timeout);
        rows.push(Row {
            label,
            value: if up { "listening".into() } else { "not listening (informational)".into() },
            health: if up { Health::Ok } else { Health::Info },
        });
    }

    // --- EVM JSON-RPC ---
    match crate::eth::chain_id(ctx) {
        Ok(cid) => rows.push(Row { label: format!("EVM RPC {}", ctx.evm_rpc), value: format!("listening (chainId 0x{cid:x})"), health: Health::Ok }),
        Err(_) => rows.push(Row { label: format!("EVM RPC {}", ctx.evm_rpc), value: "not reachable (EVM lane / --evm-rpc-listen off?)".into(), health: Health::Info }),
    }

    if let Some(daa) = virtual_daa {
        rows.push(Row { label: "Virtual DAA score".into(), value: daa.to_string(), health: Health::Ok });
    }

    // --- render ---
    match ctx.output {
        OutputFormat::Json => {
            let arr: Vec<_> = rows
                .iter()
                .map(|r| serde_json::json!({ "check": r.label, "value": r.value, "status": r.health.tag() }))
                .collect();
            println!(
                "{}",
                serde_json::json!({ "ok": exit_code == exit::SUCCESS, "exitCode": exit_code, "network": ctx.network, "syncPhase": synced_phase, "checks": arr })
            );
        }
        OutputFormat::Human => {
            let w = rows.iter().map(|r| r.label.len()).max().unwrap_or(20).max(20);
            for r in &rows {
                println!("{:<w$}  {:<48}  {}", r.label, r.value, r.health.tag(), w = w);
            }
            if !ctx.quiet {
                println!();
                println!("{}", if exit_code == exit::SUCCESS { "doctor: OK" } else { "doctor: issues found (see FAIL/WARN above)" });
                println!("note: ports gRPC/json/P2P are derived from the borsh port ({borsh_port}); deployments may rebind any of them — the wRPC getServerInfo above is authoritative.");
            }
        }
    }

    if exit_code == exit::SUCCESS {
        Ok(())
    } else {
        let reason = match exit_code {
            x if x == exit::CONNECTION => "cannot reach the node wRPC",
            x if x == exit::NETWORK_MISMATCH => "node network does not match --network",
            x if x == exit::NODE_NOT_SYNCED => "node is not synced",
            _ => "node doctor found issues",
        };
        Err(CliError::new(exit_code, format!("node doctor: {reason} (see report)")))
    }
}
