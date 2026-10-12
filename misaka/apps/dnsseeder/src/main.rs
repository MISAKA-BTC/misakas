//! MISAKA (kaspa-pq) DNS seeder.
//!
//! A Kaspa-style DNS seeder: it serves the IPs of live kaspa-pq peers over DNS so a fresh node
//! bootstraps by resolving `seeder{1,2}.misakascan.com` (its `dns_seeders` list) and randomly
//! dialing the returned peers. The live peer set is taken from a co-located node's address
//! manager over wRPC (`getPeerAddresses`). Configured `--anchors` are advertised only after a P2P
//! handshake confirms the network, genesis, consensus fingerprints, and fence schedule expected by
//! `--network-id`. The operator delegates the subdomain to this host with an NS record; this process
//! is authoritative for it and answers A queries with a random subset of the verified live set.
//!
//! Run (port 53 needs root or `setcap cap_net_bind_service=+ep`):
//!   misaka-dnsseeder --network-id testnet-10 --anchors 160.16.131.119,95.111.236.186
//! (`--network-id` derives the co-located node's Borsh port; pass `--node-wrpc-borsh host:port`
//! to override.)

use clap::Parser;
use kaspa_consensus_core::{
    config::params::Params,
    fork_id_v1::fork_id_v1,
    network::{EndpointKind, NetworkId, NetworkType},
};
use kaspa_core::{info, warn};
use kaspa_p2p_lib::{
    Adaptor, ConnectionInitializer, Hub, KaspadHandshake, Router, common::ProtocolError, convert::model::version::Version,
};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_utils::networking::PeerId;
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
use tokio::sync::Mutex;
use uuid::Uuid;

const ANCHOR_PROBE_PROTOCOL_VERSION: u32 = 105;

#[derive(Clone, Debug, PartialEq, Eq)]
struct AnchorIdentity {
    network: String,
    genesis_hash: Vec<u8>,
    consensus_params_id: Vec<u8>,
    consensus_identity_id: Vec<u8>,
    consensus_schedule_id: Vec<u8>,
}

impl AnchorIdentity {
    fn from_params(params: &Params) -> Self {
        Self {
            network: params.network_name(),
            genesis_hash: params.genesis.hash.as_bytes().to_vec(),
            consensus_params_id: params.consensus_params_id().as_bytes().to_vec(),
            consensus_identity_id: params.consensus_identity_id().as_bytes().to_vec(),
            consensus_schedule_id: params.consensus_schedule_id().as_bytes().to_vec(),
        }
    }
}

struct AnchorProbeInitializer {
    network: String,
    genesis_hash: Vec<u8>,
    consensus_params_id: Vec<u8>,
    consensus_identity_id: Vec<u8>,
    consensus_schedule_id: Vec<u8>,
    fork_id_fired: Vec<u8>,
    fork_id_next: u64,
    peer_version: Arc<Mutex<Option<Version>>>,
}

impl AnchorProbeInitializer {
    fn new(params: &Params, peer_version: Arc<Mutex<Option<Version>>>) -> Self {
        let fork_id = fork_id_v1(params, 0);
        Self {
            network: params.network_name(),
            genesis_hash: params.genesis.hash.as_bytes().to_vec(),
            consensus_params_id: params.consensus_params_id().as_bytes().to_vec(),
            consensus_identity_id: params.consensus_identity_id().as_bytes().to_vec(),
            consensus_schedule_id: params.consensus_schedule_id().as_bytes().to_vec(),
            fork_id_fired: fork_id.fired.as_bytes().to_vec(),
            fork_id_next: fork_id.next,
            peer_version,
        }
    }

    fn local_version(&self) -> Version {
        Version::new(
            None,
            PeerId::new(Uuid::new_v4()),
            self.network.clone(),
            None,
            ANCHOR_PROBE_PROTOCOL_VERSION,
            self.genesis_hash.clone(),
            self.consensus_params_id.clone(),
            self.consensus_identity_id.clone(),
            self.consensus_schedule_id.clone(),
            self.fork_id_fired.clone(),
            self.fork_id_next,
        )
    }
}

#[async_trait::async_trait]
impl ConnectionInitializer for AnchorProbeInitializer {
    async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
        let mut handshake = KaspadHandshake::new(&router);
        router.start();
        let peer_message = handshake.handshake(self.local_version().into()).await?;
        let peer_version = Version::try_from(peer_message).map_err(|error| ProtocolError::OtherOwned(error.to_string()))?;
        router.set_identity(peer_version.id);
        handshake.exchange_ready_messages().await?;
        *self.peer_version.lock().await = Some(peer_version);
        Ok(())
    }
}

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
    /// Serve ONLY the `--anchors` (skip the co-located node's address-manager peers). Each anchor is
    /// health-gated on a P2P handshake and must match the network, genesis, consensus fingerprints,
    /// and fence schedule derived from `--network-id`.
    ///
    /// In this mode the backing node is BEST-EFFORT and cannot veto them: it is evidence about the
    /// address-manager peer list, which is not served here, and about nothing else. A host that
    /// serves a delegated nameserver but cannot reach the network it points at — filtered egress,
    /// no node of its own — cannot answer until it can reach an anchor for this verification.
    #[arg(long, default_value_t = false)]
    anchors_only: bool,
}

fn parse_anchors(s: &str) -> Vec<Ipv4Addr> {
    s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect()
}

fn bytes_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_anchor_peer(peer: &Version, expected: &AnchorIdentity) -> Result<(), String> {
    if peer.network != expected.network {
        return Err(format!("network mismatch: expected {}, got {}", expected.network, peer.network));
    }
    for (label, actual, wanted) in [
        ("genesis", &peer.genesis_hash, &expected.genesis_hash),
        ("consensus params id", &peer.consensus_params_id, &expected.consensus_params_id),
        ("consensus identity id", &peer.consensus_identity_id, &expected.consensus_identity_id),
        ("fence schedule id", &peer.consensus_schedule_id, &expected.consensus_schedule_id),
    ] {
        if actual != wanted {
            return Err(format!("{label} mismatch: expected {}, got {}", bytes_hex(wanted), bytes_hex(actual)));
        }
    }
    Ok(())
}

async fn probe_anchor(anchor: Ipv4Addr, p2p_port: u16, params: &Params, expected: &AnchorIdentity) -> Result<(), String> {
    let peer_version = Arc::new(Mutex::new(None));
    let initializer = Arc::new(AnchorProbeInitializer::new(params, peer_version.clone()));
    let adaptor = Adaptor::client_only(Hub::new(), initializer, Default::default());
    let address = format!("{anchor}:{p2p_port}");
    let connect_result = adaptor.connect_peer_with_retries(address, 1, Duration::ZERO).await.map_err(|error| error.to_string());
    adaptor.close().await;
    connect_result?;
    let peer = peer_version.lock().await.take().ok_or_else(|| "handshake completed without a peer version".to_string())?;
    validate_anchor_peer(&peer, expected)
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
/// 3. Anchors must complete a P2P handshake and match the configured network's genesis,
///    consensus fingerprints, and fence schedule. A TCP-open port is not sufficient: it can be a
///    node from a different testnet generation.
///
/// What this deliberately does NOT verify (needs a P2P probe or ADR-0025's registry, recorded here
/// so nobody mistakes the gate for more than it is): the peer's own sync state, its chain identity
/// relative to a trusted checkpoint, and its ability to serve the pruning-point proof/UTXO/EVM
/// snapshots.
/// **In `--anchors-only`, the backing node cannot veto the anchors.**
///
/// The backing node remains best-effort in `--anchors-only`, but every advertised anchor is
/// independently checked against the ruleset that the seeder is configured to serve.
async fn refresh_verified(
    node_rpc: &str,
    anchors: &[Ipv4Addr],
    anchors_only: bool,
    p2p_port: u16,
    params: &Params,
    expected_identity: &AnchorIdentity,
) -> Result<Vec<Ipv4Addr>, String> {
    let url = format!("ws://{node_rpc}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None).map_err(|e| e.to_string())?;
    let backing = async {
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
        let info = client.get_server_info().await.map_err(|e| format!("get_server_info failed: {e}"))?;
        if !info.is_synced {
            return Err(format!("backing node ({}) reports is_synced=false", info.network_id));
        }
        Ok(info)
    }
    .await;

    if let Err(why) = &backing {
        if !anchors_only {
            let _ = client.disconnect().await;
            return Err(format!("{why} — refusing to advertise ANY peers"));
        }
        warn!("[dnsseeder] {why}; backing node is bypassed in anchors-only mode, but anchors still require a T12 P2P identity check");
    }

    // Gate 2 input: the peers the backing node is actually connected to right now.
    //
    // Only ever READ under `!anchors_only`, so it is only ever ASKED for there. Asking anyway was
    // the second half of the same mistake as the sync gate: a question whose answer this mode
    // discards, failing the whole refresh when the node cannot answer it.
    let connected: BTreeSet<Ipv4Addr> = if anchors_only {
        BTreeSet::new()
    } else {
        match client.get_connected_peer_info().await {
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
        }
    };

    let mut set: BTreeSet<Ipv4Addr> = BTreeSet::new();

    // Gate 3: anchors — TCP liveness alone is unsafe because testnet-11 and testnet-12 share the
    // P2P port. Require the full handshake and exact identity/schedule match before advertising.
    let mut rejected: Vec<(Ipv4Addr, String)> = Vec::new();
    for anchor in anchors {
        match tokio::time::timeout(Duration::from_secs(8), probe_anchor(*anchor, p2p_port, params, expected_identity)).await {
            Ok(Ok(_)) => {
                set.insert(*anchor);
            }
            Ok(Err(error)) => rejected.push((*anchor, error)),
            Err(_) => rejected.push((*anchor, "P2P ruleset probe timed out".to_string())),
        }
    }
    for (anchor, reason) in &rejected {
        warn!("[dnsseeder] anchor {anchor}:{p2p_port} failed T12 P2P identity/ruleset verification: {reason}");
    }
    if set.is_empty() && !anchors.is_empty() {
        warn!("[dnsseeder] no configured anchors passed P2P identity/ruleset verification — serving EMPTY answers");
    }

    if !anchors_only {
        // Unreachable here only when `anchors_only` is false, which the branch above already
        // returned on — so the node is connected and synced.
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
    let network = match args.network_id.as_deref() {
        Some(value) => NetworkId::from_str(value).unwrap_or_else(|error| {
            warn!("[dnsseeder] invalid --network-id {value}: {error}; falling back to devnet identity checks");
            NetworkId::new(NetworkType::Devnet)
        }),
        None => NetworkId::new(NetworkType::Devnet),
    };
    let params = Params::from(network);
    let expected_identity = AnchorIdentity::from_params(&params);
    let p2p_port = network.default_p2p_port();
    info!("[dnsseeder] co-located node wRPC Borsh: {node_rpc}");
    info!(
        "[dnsseeder] anchor verification identity: network={}, p2p_port={}, schedule={:?}",
        network,
        p2p_port,
        params.fence_schedule_v1()
    );
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
        let params = params.clone();
        let expected_identity = expected_identity.clone();
        let poll = Duration::from_secs(args.poll_secs.max(5));
        tokio::spawn(async move {
            loop {
                match refresh_verified(&node_rpc, &anchors, anchors_only, p2p_port, &params, &expected_identity).await {
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

    #[test]
    fn anchor_identity_rejects_a_different_fence_schedule() {
        let network = NetworkId::with_suffix(NetworkType::Testnet, 12);
        let params = Params::from(network);
        assert_eq!(params.fence_schedule_v1(), vec![750, 1000, 1300]);
        let expected = AnchorIdentity::from_params(&params);
        let mut peer = Version::new(
            None,
            PeerId::new(Uuid::new_v4()),
            expected.network.clone(),
            None,
            ANCHOR_PROBE_PROTOCOL_VERSION,
            expected.genesis_hash.clone(),
            expected.consensus_params_id.clone(),
            expected.consensus_identity_id.clone(),
            expected.consensus_schedule_id.clone(),
            Vec::new(),
            u64::MAX,
        );
        peer.consensus_schedule_id[0] ^= 1;
        let error = validate_anchor_peer(&peer, &expected).unwrap_err();
        assert!(error.contains("fence schedule id"));
    }
}
