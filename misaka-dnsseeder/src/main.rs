//! MISAKA (kaspa-pq) DNS seeder.
//!
//! A Kaspa-style DNS seeder: it serves the IPs of live kaspa-pq peers over DNS so a fresh node
//! bootstraps testnet-200 by resolving `seeder{1,2}.misakascan.com` (its `dns_seeders` list) and randomly
//! dialing the returned peers. The live peer set is taken from a co-located node's address
//! manager over wRPC (`getPeerAddresses`), always augmented with the configured `--anchors` (the
//! seed nodes) so the seeder is useful from genesis, before the network has grown. The operator
//! delegates the subdomain to this host with an NS record; this process is authoritative for it
//! and answers A queries with a random subset of the live set.
//!
//! Run (port 53 needs root or `setcap cap_net_bind_service=+ep`):
//!   misaka-dnsseeder --network-id testnet-200 --anchors 160.16.131.119,95.111.236.186
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
    /// Network id (e.g. testnet-200) the co-located node serves. Used to derive the default
    /// node wRPC Borsh port when `--node-wrpc-borsh` is not given (testnet-200 => 127.0.0.1:27210),
    /// after first consulting the local endpoint registry (~/.misaka/<net>/endpoints.json) the
    /// node wrote. The public-network default is testnet-200.
    #[arg(long = "network-id", visible_alias = "network", env = "MISAKA_NETWORK", default_value = "testnet-200")]
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
    /// Serve only the operator anchors. Use this when the node address manager
    /// contains historical or non-publicly-reachable peers.
    #[arg(long, env = "MISAKA_SEEDER_ANCHORS_ONLY", default_value_t = false)]
    anchors_only: bool,
    /// Max A records per response (a random subset of the live set).
    #[arg(long, default_value_t = 8)]
    max_answers: usize,
    /// TTL (seconds) for served A records.
    #[arg(long, default_value_t = 30)]
    ttl: u32,
    /// Seconds between refreshing the peer set from the node.
    #[arg(long, default_value_t = 30)]
    poll_secs: u64,
}

fn parse_anchors(s: &str) -> Vec<Ipv4Addr> {
    s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect()
}

/// Resolve the co-located node's wRPC Borsh endpoint: explicit `--node-wrpc-borsh` wins; else
/// derive from `--network-id` via the local endpoint registry the node wrote (registry > network
/// default); else the testnet-200 Borsh fallback. Mirrors the validator/miner resolver so the
/// whole tool-set agrees on one port-derivation rule.
/// kaspa-pq **ADR-0040 §T-shared — networks a public DNS seeder must REFUSE to serve.**
///
/// The PALW presets (`testnet-palw` = suffix 110, `devnet-palw` = 111) remain
/// closed/allowlisted experiments. A seeder is precisely the mechanism that
/// hands a network to third parties, so serving one would cross that boundary.
/// testnet-200 is deliberately not on this list: its peer allowlist is open and
/// it is now the public network.
///
/// The seeder rejects these networks explicitly rather than relying on absent configuration.
const SEEDER_REFUSED_NET_SUFFIXES: &[(&str, u32)] = &[("testnet-palw", 110), ("devnet-palw", 111)];

/// Reject a PALW network id outright. Returns the human-readable reason when refused.
fn seeder_refuses_network(network: &Option<String>) -> Option<String> {
    let net = network.as_ref()?;
    let nid = NetworkId::from_str(net).ok()?;
    let suffix = nid.suffix()?;
    SEEDER_REFUSED_NET_SUFFIXES.iter().find(|(_, s)| *s == suffix).map(|(name, s)| {
        format!(
            "refusing to serve {net}: {name} (netsuffix {s}) is an ADR-0040 closed/allowlisted PALW \
             experiment. A DNS seeder would make it publicly discoverable. Use testnet-200 for the \
             public algo-4 network, or keep this preset behind its explicit peer allowlist."
        )
    })
}

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

/// Best-effort refresh: query the co-located node's address manager and merge IPv4 peers with the
/// anchors. Errors are non-fatal (the caller keeps the last good set / anchors).
async fn refresh_peers(node_rpc: &str, anchors: &[Ipv4Addr]) -> Result<Vec<Ipv4Addr>, String> {
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

    let mut set: BTreeSet<Ipv4Addr> = anchors.iter().copied().collect();
    let resp = client.get_peer_addresses().await.map_err(|e| e.to_string());
    let _ = client.disconnect().await;
    for a in resp?.known_addresses {
        if let IpAddr::V4(v4) = a.ip.0 {
            // Audit H-01: only advertise publicly-routable peers (drop bogon Sybil).
            if is_routable_v4(&v4) {
                set.insert(v4);
            }
        }
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
    // ADR-0040 §T-shared: refuse PALW networks explicitly, before any listener binds.
    if let Some(reason) = seeder_refuses_network(&args.network_id) {
        eprintln!("[dnsseeder] {reason}");
        std::process::exit(1);
    }
    let node_rpc = resolve_node_rpc(&args.network_id, &args.node_rpc);
    info!("[dnsseeder] co-located node wRPC Borsh: {node_rpc}");
    let peers: Arc<RwLock<Vec<Ipv4Addr>>> = Arc::new(RwLock::new(anchors.clone()));

    // Background poller: refresh the live peer set from the co-located node.
    if args.anchors_only {
        info!("[dnsseeder] anchors-only mode: node address-manager discovery disabled");
    } else {
        let peers = peers.clone();
        let node_rpc = node_rpc.clone();
        let anchors = anchors.clone();
        let poll = Duration::from_secs(args.poll_secs.max(5));
        tokio::spawn(async move {
            loop {
                match refresh_peers(&node_rpc, &anchors).await {
                    Ok(ips) => {
                        let n = ips.len();
                        *peers.write().unwrap() = ips;
                        info!("[dnsseeder] peer set refreshed: {n} IPv4 peers (incl. {} anchors)", anchors.len());
                    }
                    Err(e) => {
                        // Keep serving the last good set (>= anchors); the node may just be down.
                        let mut g = peers.write().unwrap();
                        if g.len() < anchors.len() {
                            *g = anchors.clone();
                        }
                        warn!("[dnsseeder] node refresh failed ({e}); serving {} cached/anchor peers", g.len());
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
    fn public_network_defaults_are_testnet_200() {
        let args = Args::try_parse_from(["misaka-dnsseeder"]).unwrap();
        assert_eq!(args.network_id.as_deref(), Some("testnet-200"));
        assert!(!args.anchors_only);

        let args = Args::try_parse_from(["misaka-dnsseeder", "--anchors-only"]).unwrap();
        assert!(args.anchors_only);
    }

    /// ADR-0040 §T-shared — the seeder REFUSES closed PALW networks, and the refusal is a rule with an
    /// enforcement point rather than an operator convention.
    ///
    /// "Just don't list it" is the absence of a configuration: it survives exactly until someone passes
    /// `--network-id testnet-110`. A seeder is what turns a closed net into a shared one, so on the two
    /// allowlisted presets it must refuse to start at all. testnet-200 is the explicit public exception.
    #[test]
    fn seeder_refuses_palw_networks() {
        for net in ["testnet-110", "devnet-111"] {
            let reason = seeder_refuses_network(&Some(net.to_string()));
            assert!(reason.is_some(), "{net} is a PALW network and must be refused");
            assert!(reason.unwrap().contains("ADR-0040"), "the refusal must say WHY, not just fail");
        }
        // The released public PALW network is allowed; the refusal remains targeted
        // to the two closed presets.
        for net in ["testnet-200", "testnet-10", "mainnet", "devnet"] {
            assert!(seeder_refuses_network(&Some(net.to_string())).is_none(), "{net} must still be servable");
        }
        // No network id ⇒ nothing to refuse (the endpoint is explicit).
        assert!(seeder_refuses_network(&None).is_none());
    }

    #[test]
    fn resolve_node_rpc_explicit_and_fallback() {
        // explicit --node-wrpc-borsh / env wins over the network
        assert_eq!(resolve_node_rpc(&Some("testnet-200".into()), &Some("1.2.3.4:9".into())), "1.2.3.4:9");
        // no network + no explicit → the public testnet-200 Borsh fallback
        assert_eq!(resolve_node_rpc(&None, &None), "127.0.0.1:27210");
        // an unparseable network-id with no explicit → fallback (never panics)
        assert_eq!(resolve_node_rpc(&Some("bogus-net".into()), &None), "127.0.0.1:27210");
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
