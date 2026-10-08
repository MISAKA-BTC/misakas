//! **C4 round 4, F-C4R4-09: one front-running outbound peer must not hold a node on its own chain** (the N2 partition watchdog).
//!
//! What the relay flow feeds the watchdog at `2d6d77f7c`:
//!
//! 1. Only the FIRST deliverer of a block is noted. `HandleRelayInvsFlow` skips an inv whose block is already known
//!    ("already exists, continuing") or already requested from another peer (`request_block` → `None`); only the flow that obtained
//!    and processed the block calls `observe_relay_for_partition`.
//! 2. "Ours" is `is_chain_ancestor_of(sink, block)`: a block beside the sink (a sibling this node's chain merges) is `Other`, even from
//!    an honest peer.
//!
//! So an attacker-run outbound peer, ten minutes connected, that announces every new chain block first collects every `ExtendsOurs`
//! note, the honest outbound peers — which announce the same blocks a moment later — are noted only for the siblings they happen to
//! deliver first, and any refusal run holds the node. The same peer keeps a verified resync pending.
//!
//! The scenario: a chain block every second, delivered first by the front-runner and announced by every honest peer; a sibling every
//! ten seconds, delivered by one honest peer; over thirty minutes; a refusal run of 40 ticks. SAFE: no hold, and a resync on this chain
//! is confirmed.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::PalwPartitionRefusalV1;
use kaspa_p2p_flows::flowcontext::partition_watch::{PartitionVerdict, PartitionWatch, PeerView, RelayedRelation, ResyncVerdict};
use kaspa_p2p_lib::PeerKey;
use kaspa_utils::networking::{IpAddress, PeerId};
use std::net::IpAddr;
use std::time::{Duration, Instant};
use uuid::Uuid;

const HONEST: u16 = 8;
const FRONT_RUNNER: u16 = 100;

fn key(n: u16) -> PeerKey {
    PeerKey::new(PeerId::new(Uuid::from_u128(n as u128 + 1)), IpAddress::new(IpAddr::from([10, 3, (n >> 8) as u8, n as u8])))
}

fn peers() -> Vec<PeerView> {
    let mut v: Vec<PeerView> =
        (0..HONEST).map(|n| PeerView { key: key(n), outbound: true, connected_for: Duration::from_secs(3600) }).collect();
    v.push(PeerView { key: key(FRONT_RUNNER), outbound: true, connected_for: Duration::from_secs(900) });
    v
}

fn refusal() -> Option<PalwPartitionRefusalV1> {
    Some(PalwPartitionRefusalV1 {
        sink: BlockHash::from_u64_word(1),
        refused: BlockHash::from_u64_word(2),
        own_daa: 500,
        refused_daa: 520,
        first_refused_daa: 480,
        resolves: 30,
    })
}

/// One relayed block as the network shows it: who delivers it first, who announces it after, and where it stands.
struct Relay {
    hash: BlockHash,
    first: PeerKey,
    announcers: Vec<PeerKey>,
    /// On this node's selected chain (a child of the sink).
    on_our_chain: bool,
}

/// The traffic of `minutes`: a chain block a second (front-runner first, every honest peer announcing it), and a sibling every ten
/// seconds (one honest peer first, nobody else).
fn traffic(minutes: u64) -> Vec<(u64, Relay)> {
    let mut out = Vec::new();
    for s in 0..minutes * 60 {
        out.push((
            s,
            Relay {
                hash: BlockHash::from_u64_word(1_000_000 + s),
                first: key(FRONT_RUNNER),
                announcers: (0..HONEST).map(key).collect(),
                on_our_chain: true,
            },
        ));
        if s % 10 == 0 {
            out.push((
                s,
                Relay {
                    hash: BlockHash::from_u64_word(2_000_000 + s),
                    first: key((s / 10) as u16 % HONEST),
                    announcers: vec![],
                    on_our_chain: false,
                },
            ));
        }
    }
    out
}

/// **The relay flow at `2d6d77f7c`**: the first deliverer is noted, against `is_chain_ancestor_of(sink, block)`; an announcement of a
/// block already known or already requested is skipped.
fn drive(w: &mut PartitionWatch, minutes: u64, now: Instant) {
    let start = now.checked_sub(Duration::from_secs(minutes * 60)).expect("the monotonic clock is older than the window");
    for (s, r) in traffic(minutes) {
        let at = start + Duration::from_secs(s);
        let relation = if r.on_our_chain { RelayedRelation::ExtendsOurs } else { RelayedRelation::Other };
        w.note_relay(r.first, relation, at);
        let _ = (r.hash, r.announcers); // skipped by the flow
    }
}

#[test]
#[ignore = "FAIL F-C4R4-09: first-deliverer crediting and sibling-as-other let one outbound peer hold the node"]
fn a_front_running_outbound_peer_does_not_hold_a_node_on_its_own_chain() {
    let now = Instant::now();
    let mut w = PartitionWatch::default();
    drive(&mut w, 30, now);
    let verdict = w.verdict(refusal(), &peers(), now);
    assert!(matches!(verdict, PartitionVerdict::Clear(_)), "held on its own chain: {verdict:?}");
}

#[test]
#[ignore = "FAIL F-C4R4-09: the front-runner takes every ExtendsOurs note, so a resync on the network's chain is never confirmed"]
fn a_front_running_outbound_peer_does_not_keep_a_verified_resync_pending() {
    let now = Instant::now();
    let mut w = PartitionWatch::default();
    drive(&mut w, 30, now);
    let verdict = w.resync_verdict(None, &peers(), now);
    assert!(matches!(verdict, ResyncVerdict::Confirmed { .. }), "never confirmed: {verdict:?}");
}
