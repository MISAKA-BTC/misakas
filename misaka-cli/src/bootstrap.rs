//! `misaka node endpoints` / `misaka bootstrap seeds|resolve` — bootstrap visibility
//! (design §8). Read-only: surface the local endpoint registry, the DNS seeds, and the
//! resolved peer IPs FOR DEBUGGING. The normal user never needs to see seed / peer / port;
//! these commands exist so an operator can inspect them when something is wrong.

use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use std::net::ToSocketAddrs;
use std::str::FromStr;

fn parse_net(network: &str) -> Result<NetworkId, CliError> {
    NetworkId::from_str(network).map_err(|e| CliError::new(exit::GENERIC, format!("invalid network-id '{network}': {e}")))
}

/// `misaka node endpoints` — the effective node RPC endpoints: the local registry the node
/// wrote, else the network-id defaults (with a hint that the node may not be running).
pub fn endpoints(output: OutputFormat, network: &str) -> CliResult {
    let nid = parse_net(network)?;
    let reg = misaka_endpoints::EndpointRegistry::load(network);
    let grpc = misaka_endpoints::resolve(&nid, EndpointKind::NodeGrpc, None, reg.as_ref());
    let borsh = misaka_endpoints::resolve(&nid, EndpointKind::NodeWrpcBorsh, None, reg.as_ref());
    // wRPC JSON / EVM are optional: show them only if the registry recorded them.
    let reg_only = |kind: EndpointKind| reg.as_ref().and_then(|r| r.endpoints.get(kind).map(str::to_string));
    let json = reg_only(EndpointKind::NodeWrpcJson);
    let evm = reg_only(EndpointKind::EvmRpcHttp);
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "network_id": network,
                "registry": misaka_endpoints::registry_path(network).map(|p| p.display().to_string()),
                "registry_present": reg.is_some(),
                "node_grpc": grpc,
                "node_wrpc_borsh": borsh,
                "node_wrpc_json": json,
                "evm_rpc_http": evm,
            })
        ),
        OutputFormat::Human => {
            let src =
                if reg.is_some() { "(from endpoint registry)" } else { "(network defaults — no registry; is the node running?)" };
            println!("network-id: {network}  {src}");
            println!("  node-grpc:       {grpc}   miner");
            println!("  node-wrpc-borsh: {borsh}   validator / wallet");
            println!("  node-wrpc-json:  {}   explorer", json.as_deref().unwrap_or("disabled"));
            println!("  evm-rpc-http:    {}   Ethereum JSON-RPC", evm.as_deref().unwrap_or("disabled"));
        }
    }
    Ok(())
}

/// `misaka bootstrap seeds` — the DNS seed domains + default P2P port for the network.
pub fn seeds(output: OutputFormat, network: &str) -> CliResult {
    let nid = parse_net(network)?;
    let port = nid.default_p2p_port();
    let seeds = Params::from(nid).dns_seeders;
    match output {
        OutputFormat::Json => println!("{}", serde_json::json!({ "network_id": network, "p2p_default_port": port, "seeds": seeds })),
        OutputFormat::Human => {
            println!("network_id: {network}");
            println!("p2p_default_port: {port}");
            if seeds.is_empty() {
                println!("seeds: (none configured for this network)");
            } else {
                println!("seeds:");
                for s in seeds {
                    println!("  - {s}");
                }
            }
        }
    }
    Ok(())
}

/// `misaka bootstrap resolve` — resolve the DNS seeds to live peer IPs (debug only). The
/// seed returns A records (IPs only); the P2P port is the network default, not from DNS.
pub fn resolve(output: OutputFormat, network: &str) -> CliResult {
    let nid = parse_net(network)?;
    let port = nid.default_p2p_port();
    let seeds = Params::from(nid).dns_seeders;
    let mut peers: Vec<String> = Vec::new();
    for seed in seeds {
        // A-record lookup: resolve host:0, then stamp the network default P2P port.
        if let Ok(addrs) = (*seed, 0u16).to_socket_addrs() {
            for a in addrs {
                peers.push(format!("{}:{}", a.ip(), port));
            }
        }
    }
    peers.sort();
    peers.dedup();
    match output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({ "network_id": network, "p2p_default_port": port, "seeds": seeds, "resolved_peers": peers })
        ),
        OutputFormat::Human => {
            println!("network_id: {network}");
            println!("p2p_default_port: {port}");
            println!("seeds:");
            for s in seeds {
                println!("  - {s}");
            }
            println!("resolved_peers:");
            if peers.is_empty() {
                println!("  (none — seeds unreachable or returned no A records)");
            }
            for p in &peers {
                println!("  - {p}");
            }
        }
    }
    Ok(())
}
