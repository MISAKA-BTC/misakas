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
//!   misaka-dnsseeder --node-rpc 127.0.0.1:27610 --anchors 160.16.131.119,95.111.236.186

use clap::Parser;
use kaspa_core::{info, warn};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use rand::seq::SliceRandom;
use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

#[derive(Parser, Debug)]
#[command(name = "misaka-dnsseeder", version, about = "MISAKA (kaspa-pq) DNS seeder — serves live peer IPs over DNS")]
struct Args {
    /// Co-located node wRPC (borsh) endpoint host:port whose peer set is served (mainnet 27610,
    /// testnet-10 the same borsh port on that node). Best-effort: if unreachable, only the
    /// `--anchors` are served.
    #[arg(long, default_value = "127.0.0.1:27610", env = "MISAKA_SEEDER_NODE_RPC")]
    node_rpc: String,
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
}

fn parse_anchors(s: &str) -> Vec<Ipv4Addr> {
    s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect()
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
    let peers: Arc<RwLock<Vec<Ipv4Addr>>> = Arc::new(RwLock::new(anchors.clone()));

    // Background poller: refresh the live peer set from the co-located node.
    {
        let peers = peers.clone();
        let node_rpc = args.node_rpc.clone();
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
    let sock = UdpSocket::bind(&args.listen).await.unwrap_or_else(|e| panic!("bind DNS UDP {} failed: {e} (port 53 needs root / cap_net_bind_service)", args.listen));
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
    let tcp = TcpListener::bind(&args.listen).await.unwrap_or_else(|e| panic!("bind DNS TCP {} failed: {e} (port 53 needs root / cap_net_bind_service)", args.listen));
    info!("[dnsseeder] authoritative A-record server on udp+tcp://{} (anchors={:?}, ttl={}s, max_answers={})", args.listen, anchors, args.ttl, args.max_answers);
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
