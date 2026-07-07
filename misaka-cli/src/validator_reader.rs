//! Read-only validator views implemented directly in `misaka`.
//!
//! Mutating validator operations still shell out to `kaspa-pq-validator`; this
//! module handles `misaka validator status` so operators can inspect node,
//! bond, and DNS-finality health without needing the sidecar binary on PATH.

use std::str::FromStr;
use std::time::Duration;

use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_rpc_core::{GetStakeBondRequest, GetStakeBondsRequest, api::rpc::RpcApi};
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use serde_json::json;

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

#[derive(Debug, Default, PartialEq, Eq)]
struct StatusArgs {
    node_rpc: Option<String>,
    network: Option<String>,
    stake_bond: Option<String>,
}

fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<String, CliError> {
    *i += 1;
    args.get(*i).cloned().ok_or_else(|| CliError::new(2, format!("{flag} requires a value")))
}

fn parse_status_args(args: &[String]) -> Result<Option<StatusArgs>, CliError> {
    let Some(cmd) = args.first() else {
        return Ok(None);
    };
    if cmd != "status" {
        return Ok(None);
    }

    let mut out = StatusArgs::default();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "--help" {
            print_status_help();
            return Err(CliError::new(exit::SUCCESS, ""));
        } else if let Some(v) =
            arg.strip_prefix("--node-rpc=").or_else(|| arg.strip_prefix("--node-wrpc-borsh=")).or_else(|| arg.strip_prefix("--rpc="))
        {
            out.node_rpc = Some(v.to_string());
        } else if arg == "--node-rpc" || arg == "--node-wrpc-borsh" || arg == "--rpc" {
            out.node_rpc = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--network=").or_else(|| arg.strip_prefix("--network-id=")) {
            out.network = Some(v.to_string());
        } else if arg == "--network" || arg == "--network-id" {
            out.network = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--stake-bond=") {
            out.stake_bond = Some(v.to_string());
        } else if arg == "--stake-bond" {
            out.stake_bond = Some(take_value(args, &mut i, arg)?);
        } else {
            return Err(CliError::new(2, format!("unknown `misaka validator status` argument: {arg}")));
        }
        i += 1;
    }
    Ok(Some(out))
}

fn print_status_help() {
    println!(
        "Read validator/node status over node wRPC (no sidecar binary required)\n\n\
Usage: misaka validator status [OPTIONS]\n\n\
Options:\n  \
    --node-rpc <HOST:PORT>         Node wRPC Borsh endpoint (alias: --node-wrpc-borsh, --rpc)\n  \
    --network <NETWORK_ID>         Network id for default endpoint resolution (alias: --network-id)\n  \
    --stake-bond <TXID:INDEX>      Stake-bond outpoint to report\n  \
    -h, --help                     Print help\n\n\
Global options such as --output json may be passed before `validator`."
    );
}

async fn connect(network: &str, node_rpc: &Option<String>, timeout_secs: u64) -> Result<KaspaRpcClient, CliError> {
    let net = NetworkId::from_str(network).map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{network}': {e}")))?;
    let registry = misaka_endpoints::EndpointRegistry::load(network);
    let hostport = misaka_endpoints::resolve(&net, EndpointKind::NodeWrpcBorsh, node_rpc.as_deref(), registry.as_ref());
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| CliError::new(exit::CONNECTION, format!("build wRPC client: {e}")))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(timeout_secs.clamp(2, 15))),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client
        .connect(Some(options))
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("connect {url}: {e} (node up with --rpclisten-borsh?)")))?;
    Ok(client)
}

fn dns_health_name(health: u32) -> &'static str {
    match health {
        0 => "DisabledBeforeActivation",
        1 => "Active",
        2 => "DegradedStakeQualityLow",
        3 => "DegradedCertificateCensored",
        _ => "Unknown",
    }
}

async fn status(ctx: &Ctx, args: StatusArgs) -> CliResult {
    let network = args.network.as_deref().unwrap_or(&ctx.network);
    let node_rpc = args.node_rpc.or_else(|| ctx.rpc.clone());
    let client = connect(network, &node_rpc, ctx.timeout_secs).await?;
    let server = client.get_server_info().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getServerInfo failed: {e}")))?;

    if ctx.output == OutputFormat::Human {
        println!("node_network: {}", server.network_id);
        println!("node_synced:  {}", server.is_synced);
        println!("node_version: {}", server.server_version);
    }

    let bond_json = if let Some(bond) = &args.stake_bond {
        match client.get_stake_bond(GetStakeBondRequest { bond_outpoint: bond.clone() }).await {
            Ok(b) if b.available => {
                if ctx.output == OutputFormat::Human {
                    println!("bond:         {bond}");
                    println!("bond_status:  {}", b.effective_status);
                    println!("bond_amount:  {}", b.amount);
                    println!("validator_id: {}", b.validator_id);
                }
                json!({
                    "outpoint": bond,
                    "available": true,
                    "status": b.effective_status,
                    "amount": b.amount,
                    "activationDaaScore": b.activation_daa_score,
                    "validatorId": b.validator_id,
                })
            }
            Ok(_) => {
                if ctx.output == OutputFormat::Human {
                    println!("bond:         {bond} (not found in the registry)");
                }
                json!({ "outpoint": bond, "available": false })
            }
            Err(e) => {
                if ctx.output == OutputFormat::Human {
                    println!("bond:         query failed: {e} (does the node configure the overlay?)");
                }
                json!({ "outpoint": bond, "available": false, "error": e.to_string() })
            }
        }
    } else {
        json!(null)
    };

    let dns_resp = client.get_dns_confirmation().await;
    let dns_json = match dns_resp {
        Ok(d) if d.available => {
            let health = dns_health_name(d.health);
            if ctx.output == OutputFormat::Human {
                println!("dns_confirmed: {}", d.dns_confirmed);
                println!("pow_confirmed: {}", d.pow_confirmed);
                println!("work_depth:    {}/{}", d.work_depth, d.required_work_depth);
                println!("stake_depth:   {}/{}", d.stake_depth, d.required_stake_depth);
                println!("dns_health:    {health}");
                println!("dns_anchor:    {} (daa {})", d.last_dns_confirmed_anchor, d.last_dns_confirmed_anchor_daa_score);
            }
            json!({
                "available": true,
                "dnsConfirmed": d.dns_confirmed,
                "powConfirmed": d.pow_confirmed,
                "workDepth": d.work_depth,
                "requiredWorkDepth": d.required_work_depth,
                "stakeDepth": d.stake_depth,
                "requiredStakeDepth": d.required_stake_depth,
                "health": health,
                "healthCode": d.health,
                "anchor": d.last_dns_confirmed_anchor,
                "anchorDaaScore": d.last_dns_confirmed_anchor_daa_score,
            })
        }
        Ok(_) => {
            if ctx.output == OutputFormat::Human {
                println!("dns:          overlay not active on this node");
            }
            json!({ "available": false })
        }
        Err(e) => {
            if ctx.output == OutputFormat::Human {
                println!("dns:          query failed: {e}");
            }
            json!({ "available": false, "error": e.to_string() })
        }
    };

    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            json!({
                "ok": true,
                "expectedNetwork": network,
                "node": {
                    "network": server.network_id.to_string(),
                    "synced": server.is_synced,
                    "version": server.server_version,
                    "virtualDaaScore": server.virtual_daa_score,
                    "utxoIndex": server.has_utxo_index,
                },
                "bond": bond_json,
                "dns": dns_json,
            })
        );
    }

    let _ = client.disconnect().await;
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
struct BondsArgs {
    node_rpc: Option<String>,
    network: Option<String>,
    owner: Option<String>,
    status: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
    all: bool,
}

fn parse_bonds_args(args: &[String]) -> Result<Option<BondsArgs>, CliError> {
    let Some(cmd) = args.first() else {
        return Ok(None);
    };
    if cmd != "bonds" {
        return Ok(None);
    }

    let mut out = BondsArgs::default();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "--help" {
            print_bonds_help();
            return Err(CliError::new(exit::SUCCESS, ""));
        } else if let Some(v) =
            arg.strip_prefix("--node-rpc=").or_else(|| arg.strip_prefix("--node-wrpc-borsh=")).or_else(|| arg.strip_prefix("--rpc="))
        {
            out.node_rpc = Some(v.to_string());
        } else if arg == "--node-rpc" || arg == "--node-wrpc-borsh" || arg == "--rpc" {
            out.node_rpc = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--network=").or_else(|| arg.strip_prefix("--network-id=")) {
            out.network = Some(v.to_string());
        } else if arg == "--network" || arg == "--network-id" {
            out.network = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--owner=") {
            out.owner = Some(v.to_string());
        } else if arg == "--owner" {
            out.owner = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--status=") {
            out.status = Some(v.to_string());
        } else if arg == "--status" {
            out.status = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--cursor=") {
            out.cursor = Some(v.to_string());
        } else if arg == "--cursor" {
            out.cursor = Some(take_value(args, &mut i, arg)?);
        } else if let Some(v) = arg.strip_prefix("--limit=") {
            out.limit = Some(v.parse().map_err(|_| CliError::new(2, format!("--limit expects a number, got '{v}'")))?);
        } else if arg == "--limit" {
            let v = take_value(args, &mut i, arg)?;
            out.limit = Some(v.parse().map_err(|_| CliError::new(2, format!("--limit expects a number, got '{v}'")))?);
        } else if arg == "--all" {
            out.all = true;
        } else {
            return Err(CliError::new(2, format!("unknown `misaka validator bonds` argument: {arg}")));
        }
        i += 1;
    }
    Ok(Some(out))
}

fn print_bonds_help() {
    println!(
        "List stake bonds over node wRPC (owner recovery of bond outpoints)\n\n\
Usage: misaka validator bonds [OPTIONS]\n\n\
Options:\n  \
    --node-rpc <HOST:PORT>         Node wRPC Borsh endpoint (alias: --node-wrpc-borsh, --rpc)\n  \
    --network <NETWORK_ID>         Network id for default endpoint resolution (alias: --network-id)\n  \
    --owner <OWNER_PUBKEY_HASH>    Filter to bonds owned by this Hash64 (hex)\n  \
    --status <LIST>                Filter by effective status, comma-separated\n  \
                                   (pending,active,unbonding,slashed)\n  \
    --limit <N>                    Max entries per page (0 = server default; capped at 1000)\n  \
    --cursor <TXID:INDEX>          Resume after this outpoint (from a previous next_cursor)\n  \
    --all                          Auto-paginate through every page\n  \
    -h, --help                     Print help\n\n\
The StakeBonds store is outpoint-keyed with no owner index, so --owner is a\n\
node-side full scan; results are always bounded by --limit and a cursor.\n\
Global options such as --output json may be passed before `validator`."
    );
}

async fn bonds(ctx: &Ctx, args: BondsArgs) -> CliResult {
    let network = args.network.as_deref().unwrap_or(&ctx.network);
    let node_rpc = args.node_rpc.clone().or_else(|| ctx.rpc.clone());
    let client = connect(network, &node_rpc, ctx.timeout_secs).await?;

    let status_in =
        args.status.as_ref().map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect::<Vec<_>>());

    let mut cursor = args.cursor.clone();
    let mut all_entries: Vec<serde_json::Value> = Vec::new();
    // `loop` always runs at least once, so this is definitely assigned before the
    // post-loop read; no dead initializer.
    let mut pov_daa_score;
    // Pin page 1's pov and reuse it for every subsequent `--all` page, so a
    // status-filtered walk is a consistent snapshot (a bond whose status changed
    // mid-walk would otherwise be skipped as the sink advances between fetches).
    let mut pinned_pov: Option<u64> = None;
    // Strict-progress guard: refuse to re-request a cursor already used, so a
    // buggy/hostile node cannot spin the `--all` loop forever.
    let mut seen_cursors: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut printed_header = false;

    loop {
        let req = GetStakeBondsRequest {
            owner_pubkey_hash: args.owner.clone(),
            status_in: status_in.clone(),
            cursor: cursor.clone(),
            limit: args.limit.unwrap_or(0),
            pov_daa_score: pinned_pov,
        };
        let resp = client
            .get_stake_bonds(req)
            .await
            .map_err(|e| CliError::new(exit::GENERIC, format!("getStakeBonds failed: {e} (does the node configure the overlay?)")))?;
        pov_daa_score = resp.pov_daa_score;
        pinned_pov.get_or_insert(resp.pov_daa_score);

        if ctx.output == OutputFormat::Human {
            if !printed_header {
                println!("network:       {network}");
                printed_header = true;
            }
            for b in &resp.bonds {
                println!(
                    "{:<9} amount={:<20} outpoint={} owner={} validator={}",
                    b.effective_status, b.amount, b.bond_outpoint, b.owner_pubkey_hash, b.validator_id
                );
            }
        }
        for b in &resp.bonds {
            all_entries.push(json!({
                "outpoint": b.bond_outpoint,
                "owner": b.owner_pubkey_hash,
                "validatorId": b.validator_id,
                "amount": b.amount,
                "activationDaaScore": b.activation_daa_score,
                "unbondingPeriodBlocks": b.unbonding_period_blocks,
                "unbondRequestDaaScore": b.unbond_request_daa_score,
                "storedStatus": b.stored_status,
                "effectiveStatus": b.effective_status,
            }));
        }

        cursor = resp.next_cursor.clone();
        if !args.all || cursor.is_none() || resp.bonds.is_empty() {
            break;
        }
        // strict-progress guard: if the node hands back a cursor we've already
        // requested, it isn't advancing — stop rather than spin forever.
        if let Some(c) = &cursor
            && !seen_cursors.insert(c.clone())
        {
            if ctx.output == OutputFormat::Human {
                eprintln!("warning: node returned a non-advancing cursor; stopping --all at {} entries", all_entries.len());
            }
            break;
        }
    }

    if ctx.output == OutputFormat::Human {
        // Single pov for the whole run (pinned across `--all`), so it agrees with JSON.
        println!("pov_daa_score: {pov_daa_score}");
        if let Some(c) = &cursor
            && !args.all
        {
            println!("next_cursor:   {c}  (pass --cursor to continue, or --all)");
        }
        println!("count:         {}", all_entries.len());
    } else {
        println!(
            "{}",
            json!({
                "ok": true,
                "expectedNetwork": network,
                "povDaaScore": pov_daa_score,
                "nextCursor": cursor,
                "count": all_entries.len(),
                "bonds": all_entries,
            })
        );
    }

    let _ = client.disconnect().await;
    Ok(())
}

pub async fn maybe_handle(ctx: &Ctx, args: &[String]) -> Option<CliResult> {
    match args.first().map(String::as_str) {
        Some("status") => match parse_status_args(args) {
            Ok(Some(a)) => Some(status(ctx, a).await),
            Ok(None) => None,
            Err(e) if e.code == exit::SUCCESS && e.msg.is_empty() => Some(Ok(())),
            Err(e) => Some(Err(e)),
        },
        Some("bonds") => match parse_bonds_args(args) {
            Ok(Some(a)) => Some(bonds(ctx, a).await),
            Ok(None) => None,
            Err(e) if e.code == exit::SUCCESS && e.msg.is_empty() => Some(Ok(())),
            Err(e) => Some(Err(e)),
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn ignores_non_status_validator_commands() {
        assert_eq!(parse_status_args(&s(&["bond", "--amount", "1"])).unwrap(), None);
    }

    #[test]
    fn parses_status_aliases() {
        let parsed =
            parse_status_args(&s(&["status", "--node-rpc", "127.0.0.1:27610", "--network-id=testnet-10", "--stake-bond", "abc:0"]))
                .unwrap()
                .unwrap();
        assert_eq!(
            parsed,
            StatusArgs {
                node_rpc: Some("127.0.0.1:27610".to_string()),
                network: Some("testnet-10".to_string()),
                stake_bond: Some("abc:0".to_string()),
            }
        );
    }

    #[test]
    fn bonds_and_status_parsers_are_disjoint() {
        // `bond` (singular, mutating) must still fall through to the sidecar.
        assert_eq!(parse_bonds_args(&s(&["bond", "--amount", "1"])).unwrap(), None);
        assert_eq!(parse_status_args(&s(&["bonds"])).unwrap(), None);
    }

    #[test]
    fn parses_bonds_filters() {
        let parsed = parse_bonds_args(&s(&[
            "bonds",
            "--rpc",
            "127.0.0.1:27610",
            "--network-id=testnet-10",
            "--owner=deadbeef",
            "--status",
            "active,unbonding",
            "--limit=50",
            "--cursor",
            "abc:0",
            "--all",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed,
            BondsArgs {
                node_rpc: Some("127.0.0.1:27610".to_string()),
                network: Some("testnet-10".to_string()),
                owner: Some("deadbeef".to_string()),
                status: Some("active,unbonding".to_string()),
                cursor: Some("abc:0".to_string()),
                limit: Some(50),
                all: true,
            }
        );
    }
}
