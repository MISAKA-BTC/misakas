//! MISAKA (kaspa-pq) DNS seeder.
//!
//! A Kaspa-style DNS seeder: it serves the IPs of live kaspa-pq peers over DNS so a fresh node
//! bootstraps by resolving `seeder{1,2}.misakascan.com` (its `dns_seeders` list) and randomly
//! dialing the returned peers. The live peer set is taken from a co-located node's address
//! manager over wRPC (`getPeerAddresses`), always augmented with the configured `--anchors` (the
//! seed nodes) so the seeder is useful from genesis, before the network has grown. The operator
//! delegates the subdomain to this host with an NS record; this process is authoritative for it
//! and answers A queries with a random subset of the live set.
//!
//! Run (port 53 needs root or `setcap cap_net_bind_service=+ep`):
//!   misaka-dnsseeder --network-id testnet-10 --anchors 160.16.131.119,95.111.236.186
//! (`--network-id` derives the co-located node's Borsh port; pass `--node-wrpc-borsh host:port`
//! to override.)

use clap::Parser;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_core::{info, warn};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use rand::seq::SliceRandom;
use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[derive(Parser, Debug)]
#[command(name = "misaka-dnsseeder", version, about = "MISAKA (kaspa-pq) DNS seeder — serves live peer IPs over DNS")]
struct Args {
    /// Network id (e.g. testnet-10) the co-located node serves. Used to derive the default
    /// node wRPC Borsh port when `--node-wrpc-borsh` is not given (testnet-10 => 127.0.0.1:27210),
    /// after first consulting the local endpoint registry (~/.misaka/<net>/endpoints.json) the
    /// node wrote. Omit if you pass `--node-wrpc-borsh` explicitly.
    #[arg(long = "network-id", visible_alias = "network", env = "MISAKA_NETWORK")]
    network_id: Option<String>,
    /// Co-located node wRPC Borsh endpoint host:port whose peer set is served. Best-effort:
    /// if unreachable, only the `--anchors` are served. When omitted it is resolved from
    /// `--network-id` (registry > network default; falls back to the devnet Borsh port 27610 if
    /// neither is set). `--node-rpc` is a deprecated alias for `--node-wrpc-borsh`.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "MISAKA_SEEDER_NODE_RPC")]
    node_rpc: Option<String>,
    /// UDP bind for the DNS server. Real delegation needs port 53 (root or cap_net_bind_service).
    #[arg(long, default_value = "0.0.0.0:53", env = "MISAKA_SEEDER_LISTEN")]
    listen: String,
    /// Anchor peer IPv4s (comma-separated) ALWAYS served (the seed nodes), for bootstrap.
    #[arg(long, default_value = "", env = "MISAKA_SEEDER_ANCHORS")]
    anchors: String,
    /// Max A records per response (a random subset of the live set).
    #[arg(long, default_value_t = 8)]
    max_answers: usize,
    /// TTL (seconds) for served A records.
    #[arg(long, default_value_t = 30)]
    ttl: u32,
    /// Seconds between refreshing the peer set from the node.
    #[arg(long, default_value_t = 30)]
    poll_secs: u64,
    /// Serve ONLY the `--anchors` (skip the co-located node's address-manager peers). The anchors
    /// are still health-gated (backing-node sync + TCP liveness) like everything else.
    #[arg(long, default_value_t = false)]
    anchors_only: bool,
}

fn parse_anchors(s: &str) -> Vec<Ipv4Addr> {
    s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect()
}

/// Resolve the co-located node's wRPC Borsh endpoint: explicit `--node-wrpc-borsh` wins; else
/// derive from `--network-id` via the local endpoint registry the node wrote (registry > network
/// default); else the historical devnet Borsh fallback. Mirrors the validator/miner resolver so the
/// whole tool-set agrees on one port-derivation rule.
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
    "127.0.0.1:27610".to_string()
}

/// Audit H-01: a public seeder must serve only publicly-ROUTABLE peer IPs. Drop
/// private/loopback/link-local/CGNAT/documentation/multicast/reserved addresses so
/// an attacker who poisons the node's address store with bogon Sybil entries cannot
/// have them advertised to fresh nodes. (The operator-supplied anchors are trusted
/// and served regardless.) A stable-Rust composition of the non-global ranges.
fn is_routable_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    let cgnat = o[0] == 100 && (o[1] & 0xC0) == 64; // 100.64.0.0/10
    let ietf_protocol = o[0] == 192 && o[1] == 0 && o[2] == 0; // 192.0.0.0/24
    let reserved = o[0] >= 240; // 240.0.0.0/4 (incl. 255.255.255.255)
    let this_network = o[0] == 0; // 0.0.0.0/8
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || cgnat
        || ietf_protocol
        || reserved
        || this_network)
}

/// F5 (t10): health-gated refresh. A seeder is the network's front door, and testnet-10 showed what
/// an ungated one does: it kept advertising two anchors of which one was an isolated self-mining
/// node and the other a wedged headers-only node, so every newcomer was routed into a bootstrap
/// path that could not complete. The gate is fail-closed at every layer:
///
/// 1. The BACKING NODE must itself report `is_synced` — a seeder whose own node is unsynced/wedged
///    serves an EMPTY answer set rather than routing newcomers at a broken mesh (a resolver with no
///    answers makes the newcomer try another seeder; a poisoned answer traps it).
/// 2. Address-manager peers are advertised only if the backing node is CURRENTLY CONNECTED to them
///    — a live protocol-102 handshake on the right network is the strongest per-peer evidence this
///    process can obtain without a P2P probe stack.
/// 3. Anchors are operator-trusted for identity but still must be TCP-alive on the network's P2P
///    port.
///
/// What this deliberately does NOT verify (needs a P2P probe or ADR-0025's registry, recorded here
/// so nobody mistakes the gate for more than it is): the peer's own sync state, its chain identity
/// relative to a trusted checkpoint, and its ability to serve the pruning-point proof/UTXO/EVM
/// snapshots.
async fn refresh_verified(node_rpc: &str, anchors: &[Ipv4Addr], anchors_only: bool, p2p_port: u16) -> Result<Vec<Ipv4Addr>, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None).map_err(|e| e.to_string())?;
    client
        .connect(Some(ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(Duration::from_millis(5_000)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        }))
        .await
        .map_err(|e| e.to_string())?;

    // Gate 1: the backing node's own sync state. `get_server_info.is_synced` is the same signal
    // operators read via `node doctor`.
    let info = match client.get_server_info().await {
        Ok(i) => i,
        Err(e) => {
            let _ = client.disconnect().await;
            return Err(format!("get_server_info failed: {e}"));
        }
    };
    if !info.is_synced {
        let _ = client.disconnect().await;
        return Err(format!("backing node ({}) reports is_synced=false — refusing to advertise ANY peers", info.network_id));
    }

    // Gate 2 input: the peers the backing node is actually connected to right now.
    let connected: BTreeSet<Ipv4Addr> = match client.get_connected_peer_info().await {
        Ok(r) => r
            .peer_info
            .iter()
            .filter_map(|p| match p.address.ip.0 {
                IpAddr::V4(v4) => Some(v4),
                _ => None,
            })
            .collect(),
        Err(e) => {
            let _ = client.disconnect().await;
            return Err(format!("get_connected_peer_info failed: {e}"));
        }
    };

    let mut set: BTreeSet<Ipv4Addr> = BTreeSet::new();

    // Gate 3: anchors — operator-trusted identity, but must be alive on the P2P port.
    for anchor in anchors {
        match tokio::time::timeout(Duration::from_secs(3), tokio::net::TcpStream::connect((*anchor, p2p_port))).await {
            Ok(Ok(_)) => {
                set.insert(*anchor);
            }
            _ => warn!("[dnsseeder] anchor {anchor}:{p2p_port} is not reachable — NOT advertising it this cycle"),
        }
    }

    if !anchors_only {
        let resp = client.get_peer_addresses().await.map_err(|e| e.to_string());
        let _ = client.disconnect().await;
        for a in resp?.known_addresses {
            if let IpAddr::V4(v4) = a.ip.0 {
                // Audit H-01: only advertise publicly-routable peers (drop bogon Sybil);
                // F5: and only those the backing node has a live handshake with.
                if is_routable_v4(&v4) && connected.contains(&v4) {
                    set.insert(v4);
                }
            }
        }
    } else {
        let _ = client.disconnect().await;
    }
    Ok(set.into_iter().collect())
}

/// A random subset of up to `max` IPs.
fn pick(all: &[Ipv4Addr], max: usize) -> Vec<Ipv4Addr> {
    let mut v = all.to_vec();
    v.shuffle(&mut rand::thread_rng());
    v.truncate(max);
    v
}

/// Build a minimal authoritative DNS response: echo the question and, for an A query, append one
/// A record per IP (NAME compressed to the question's QNAME). Non-A queries get a NOERROR/0-answer
/// reply. `None` if the query is malformed.
fn build_dns_response(query: &[u8], ips: &[Ipv4Addr], ttl: u32) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }
    let rd = query[2] & 0x01; // recursion-desired bit (low bit of the flags' high byte)
    let qdcount = u16::from_be_bytes([query[4], query[5]]);
    if qdcount != 1 {
        return None;
    }
    // Walk the question's QNAME labels (no compression pointers are valid in a question).
    let mut i = 12usize;
    loop {
        if i >= query.len() {
            return None;
        }
        let len = query[i] as usize;
        if len == 0 {
            i += 1;
            break;
        }
        if len & 0xC0 != 0 {
            return None;
        }
        i += 1 + len;
    }
    if i + 4 > query.len() {
        return None;
    }
    let qtype = u16::from_be_bytes([query[i], query[i + 1]]);
    let qend = i + 4; // past QTYPE + QCLASS
    let question = &query[12..qend];

    let answers: &[Ipv4Addr] = if qtype == 1 { ips } else { &[] };

    let mut resp = Vec::with_capacity(qend + answers.len() * 16);
    resp.extend_from_slice(&query[0..2]); // echo transaction id
    resp.push(0x84 | rd); // QR=1, AA=1, RD copied
    resp.push(0x00); // RA=0, RCODE=0 (NOERROR)
    resp.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    resp.extend_from_slice(&(answers.len() as u16).to_be_bytes()); // ANCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    resp.extend_from_slice(question); // echo the question
    for ip in answers {
        resp.extend_from_slice(&[0xC0, 0x0C]); // NAME -> pointer to the QNAME at offset 12
        resp.extend_from_slice(&1u16.to_be_bytes()); // TYPE = A
        resp.extend_from_slice(&1u16.to_be_bytes()); // CLASS = IN
        resp.extend_from_slice(&ttl.to_be_bytes()); // TTL
        resp.extend_from_slice(&4u16.to_be_bytes()); // RDLENGTH
        resp.extend_from_slice(&ip.octets()); // RDATA
    }
    Some(resp)
}

#[tokio::main]
async fn main() {
    kaspa_core::log::init_logger(None, "info");
    let args = Args::parse();
    let anchors = parse_anchors(&args.anchors);
    let node_rpc = resolve_node_rpc(&args.network_id, &args.node_rpc);
    // The network's P2P port, for the anchor liveness probe.
    let p2p_port =
        args.network_id.as_deref().and_then(|n| NetworkId::from_str(n).ok()).map(|nid| nid.default_p2p_port()).unwrap_or(26611); // the historical devnet fallback, matching resolve_node_rpc
    info!("[dnsseeder] co-located node wRPC Borsh: {node_rpc}");
    if args.anchors_only {
        info!("[dnsseeder] anchors-only mode: node address-manager discovery disabled");
    }
    // F5: start EMPTY and fail-closed — nothing is advertised until the backing
    // node has been verified synced once. A seeder that answers with unverified
    // peers routes newcomers into a broken bootstrap (the t10 failure); a seeder
    // that answers with nothing makes them retry another seeder.
    let peers: Arc<RwLock<Vec<Ipv4Addr>>> = Arc::new(RwLock::new(Vec::new()));

    // Background poller: refresh the health-verified peer set from the co-located node.
    {
        let peers = peers.clone();
        let node_rpc = node_rpc.clone();
        let anchors = anchors.clone();
        let anchors_only = args.anchors_only;
        let poll = Duration::from_secs(args.poll_secs.max(5));
        tokio::spawn(async move {
            loop {
                match refresh_verified(&node_rpc, &anchors, anchors_only, p2p_port).await {
                    Ok(ips) => {
                        let n = ips.len();
                        *peers.write().unwrap() = ips;
                        info!("[dnsseeder] verified peer set refreshed: {n} IPv4 peers ({} anchors configured)", anchors.len());
                    }
                    Err(e) => {
                        // F5 fail-closed: a seeder that cannot VERIFY its backing node
                        // (down, unsynced, wedged) must not advertise anyone — the t10
                        // incident was precisely a seeder faithfully serving two broken
                        // anchors. Empty answers make resolvers try the other seeders.
                        *peers.write().unwrap() = Vec::new();
                        warn!("[dnsseeder] refresh failed ({e}); serving an EMPTY answer set until the backing node verifies healthy");
                    }
                }
                tokio::time::sleep(poll).await;
            }
        });
    }

    // UDP server (the primary DNS transport).
    let sock = UdpSocket::bind(&args.listen)
        .await
        .unwrap_or_else(|e| panic!("bind DNS UDP {} failed: {e} (port 53 needs root / cap_net_bind_service)", args.listen));
    {
        let peers = peers.clone();
        let (max, ttl) = (args.max_answers, args.ttl);
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            loop {
                let (n, src) = match sock.recv_from(&mut buf).await {
                    Ok(x) => x,
                    Err(_) => continue,
                };
                let ips = {
                    let g = peers.read().unwrap();
                    pick(&g, max)
                };
                if let Some(resp) = build_dns_response(&buf[..n], &ips, ttl) {
                    let _ = sock.send_to(&resp, src).await;
                }
            }
        });
    }

    // TCP server (RFC 1035 §4.2.2: 2-byte length-prefixed messages). Standard DNS fallback —
    // and the transport reachable when only TCP 53 is allowed through the firewall.
    let tcp = TcpListener::bind(&args.listen)
        .await
        .unwrap_or_else(|e| panic!("bind DNS TCP {} failed: {e} (port 53 needs root / cap_net_bind_service)", args.listen));
    info!(
        "[dnsseeder] authoritative A-record server on udp+tcp://{} (anchors={:?}, ttl={}s, max_answers={})",
        args.listen, anchors, args.ttl, args.max_answers
    );
    loop {
        let (mut stream, _) = match tcp.accept().await {
            Ok(x) => x,
            Err(_) => continue,
        };
        let peers = peers.clone();
        let (max, ttl) = (args.max_answers, args.ttl);
        tokio::spawn(async move {
            let mut lenbuf = [0u8; 2];
            if stream.read_exact(&mut lenbuf).await.is_err() {
                return;
            }
            let len = u16::from_be_bytes(lenbuf) as usize;
            if len == 0 || len > 4096 {
                return;
            }
            let mut q = vec![0u8; len];
            if stream.read_exact(&mut q).await.is_err() {
                return;
            }
            let ips = {
                let g = peers.read().unwrap();
                pick(&g, max)
            };
            if let Some(resp) = build_dns_response(&q, &ips, ttl) {
                let rlen = (resp.len() as u16).to_be_bytes();
                let _ = stream.write_all(&rlen).await;
                let _ = stream.write_all(&resp).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_node_rpc_explicit_and_fallback() {
        // explicit --node-wrpc-borsh / env wins over the network
        assert_eq!(resolve_node_rpc(&Some("testnet-10".into()), &Some("1.2.3.4:9".into())), "1.2.3.4:9");
        // no network + no explicit → the historical devnet Borsh fallback
        assert_eq!(resolve_node_rpc(&None, &None), "127.0.0.1:27610");
        // an unparseable network-id with no explicit → fallback (never panics)
        assert_eq!(resolve_node_rpc(&Some("bogus-net".into()), &None), "127.0.0.1:27610");
        // (the network-default + registry branches are covered by misaka_endpoints::resolve tests,
        //  which run with a controlled HOME; asserting them here would be machine-dependent)
    }

    #[test]
    fn parse_anchors_filters_junk() {
        assert_eq!(
            parse_anchors("1.2.3.4, 5.6.7.8 ,bad,, 9.9.9.9"),
            vec![Ipv4Addr::new(1, 2, 3, 4), Ipv4Addr::new(5, 6, 7, 8), Ipv4Addr::new(9, 9, 9, 9),]
        );
        assert!(parse_anchors("").is_empty());
    }
}
