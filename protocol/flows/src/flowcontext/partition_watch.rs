//! **LIVE-R1 N2: the partition watchdog** — does this node keep a branch the network has left?
//!
//! Devnet r1's B, after a 41-minute partition healed on the wire, refused every heavier majority
//! candidate on every resolve (the PALW deep-reorg rule: its own branch carried licences the majority's
//! did not) and kept minting heartbeats on its own branch. On testnet-12 that is worse than a wedge: its
//! own chain reaches `finality_depth` (360 blue score, a few hours of heartbeats) above the fork, every
//! majority candidate then fails the finality check, and the split is sealed for good. Stopping the
//! node's participation keeps its finality point where it is — the split stays healable — and tells the
//! operator. This module decides WHEN.
//!
//! # Threat model
//!
//! What the hold costs: a held node does not mine, attest or call itself synced. It never moves its
//! sink — the hold changes participation, not fork choice — so the worst a false hold does is take one
//! honest producer offline. An attacker who could trigger it on many nodes at once would have a
//! liveness attack. The inputs it could try:
//!
//! * **Forged blue work.** On testnet-12 an attempt header carries 2²⁰ of blue work at the cost of a
//!   signature (a lottery-losing attempt is coloured blue beside an honest heartbeat), so "a heavier
//!   chain is being refused" is cheap to manufacture: an attacker releases a heavy junk branch and every
//!   honest node's sink search refuses it on the PALW rule, exactly as B refused the majority. The
//!   consensus half ([`PalwPartitionRefusalV1`]) therefore CANNOT be the trigger on its own.
//! * **Sybil peers.** Many cheap connections relaying only the junk branch. Answered by counting only
//!   OUTBOUND peers — chosen by this node from its address manager — that have been connected for at
//!   least [`PARTITION_MIN_PEER_AGE`]. Inbound connections, however many, are not in the denominator or
//!   the numerator.
//! * **Header-only or unweighable chains.** The consensus half records only a candidate the sink search
//!   UTXO-validated and weighed on both sides, refused because it did not strictly out-weigh this node's
//!   sink — never a header the node holds without a body, never a chain it could not weigh.
//! * **A one-off release.** The refused branch must keep advancing while it is refused, by
//!   [`PARTITION_MIN_REFUSED_SPAN_DAA`] ticks: a private branch released once and abandoned does not.
//! * **An eclipse** (the attacker IS most of this node's long-lived outbound peers). Then the attacker
//!   already decides what this node hears; the hold is the safe failure (the node stops, it does not
//!   follow), and it reverses as soon as honest outbound peers are back in the majority.
//!
//! What "on the other chain" means for a peer, measured without trusting anything it says: in the last
//! [`PARTITION_OBSERVATION_WINDOW`] it relayed at least one block outside the future of this node's sink
//! and none inside it. An honest peer on this node's chain relays this chain's new blocks (it extends
//! the chain it is on), so during an attacker's release it still counts as OURS even though it relays
//! the junk too; a majority peer after a partition relays only the majority's blocks — and never this
//! branch's, because its relay flow skips a block below its own merge-depth root.
//!
//! Hold, all of: a refusal run whose refused branch advanced at least [`PARTITION_MIN_REFUSED_SPAN_DAA`]
//! ticks; at least [`PARTITION_MIN_OUTBOUND_ON_OTHER`] eligible outbound peers on the other chain; and
//! they are a strict majority of ALL eligible outbound peers (a silent peer counts against the hold).
//! Release: the moment any of it stops being true — the evaluation runs on every relayed block.
use kaspa_consensus_core::api::PalwPartitionRefusalV1;
use kaspa_p2p_lib::PeerKey;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long an outbound connection must have lasted before its peer counts. Longer than a reconnect
/// churn and than the time an attacker's freshly accepted address would take to be dialled.
pub const PARTITION_MIN_PEER_AGE: Duration = Duration::from_secs(10 * 60);
/// How far back a peer's relays are remembered. Several testnet-12 slots, so a quiet minute does not
/// flip a peer.
pub const PARTITION_OBSERVATION_WINDOW: Duration = Duration::from_secs(30 * 60);
/// How many DAA ticks the refused branch must advance while it is refused: the network's clock moving
/// on without this node, not one release.
pub const PARTITION_MIN_REFUSED_SPAN_DAA: u64 = 10;
/// Never hold on the word of a single outbound peer.
pub const PARTITION_MIN_OUTBOUND_ON_OTHER: usize = 2;

/// Where a relayed block stands relative to this node's sink when it was processed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayedRelation {
    /// In the future of this node's sink: the peer extends the chain this node is on.
    ExtendsOurs,
    /// Anywhere else: a branch this node is not on.
    Other,
}

/// What the watchdog knows about a connected peer, read off the hub.
#[derive(Clone, Copy, Debug)]
pub struct PeerView {
    pub key: PeerKey,
    pub outbound: bool,
    pub connected_for: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
struct Observation {
    last_ours: Option<Instant>,
    last_other: Option<Instant>,
}

/// The watchdog's answer, with the numbers that produced it (for the operator's log line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartitionVerdict {
    Hold { refusal: PalwPartitionRefusalV1, on_other: usize, on_ours: usize, eligible: usize },
    Clear(ClearReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearReason {
    /// The last resolve refused nothing on the PALW rule.
    NoRefusal,
    /// The refused branch has not advanced far enough while refused.
    RefusalTooShort { span_daa: u64 },
    /// Too few long-lived outbound peers are on the other chain, or they are not a majority.
    PeersNotOnTheOtherChain { on_other: usize, on_ours: usize, eligible: usize },
}

/// **LIVE-R1: the verified resync** (`kaspad --palw-verified-resync`). The operator's remedy for a node
/// the watchdog held: the data directory is moved aside (never deleted), the node syncs from an empty one
/// — the pruning proof, every body after it and the PALW fold, validated by this node — and stays held
/// until long-lived outbound peers confirm the chain it synced. Never automatic: only the operator starts
/// it, so an attacker cannot use it to move a node; and an eclipsed node that synced an attacker's branch
/// stays held (its honest outbound peers relay another chain) with its old data directory beside it.
pub const RESYNC_MIN_CONFIRMING_PEERS: usize = 2;

/// The verified resync's answer: whether the chain this node synced is confirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResyncVerdict {
    Confirmed { on_ours: usize, eligible: usize },
    Pending { on_ours: usize, on_other: usize, eligible: usize, refusing: bool },
}

#[derive(Debug, Default)]
pub struct PartitionWatch {
    peers: HashMap<PeerKey, Observation>,
}

impl PartitionWatch {
    pub fn note_relay(&mut self, peer: PeerKey, relation: RelayedRelation, now: Instant) {
        let o = self.peers.entry(peer).or_default();
        match relation {
            RelayedRelation::ExtendsOurs => o.last_ours = Some(now),
            RelayedRelation::Other => o.last_other = Some(now),
        }
    }

    pub fn forget_peer(&mut self, peer: &PeerKey) {
        self.peers.remove(peer);
    }

    /// The verdict for `refusal` (the consensus half) and the connected `peers`, at `now`.
    pub fn verdict(&self, refusal: Option<PalwPartitionRefusalV1>, peers: &[PeerView], now: Instant) -> PartitionVerdict {
        let Some(refusal) = refusal else { return PartitionVerdict::Clear(ClearReason::NoRefusal) };
        let span_daa = refusal.refused_span_daa();
        if span_daa < PARTITION_MIN_REFUSED_SPAN_DAA {
            return PartitionVerdict::Clear(ClearReason::RefusalTooShort { span_daa });
        }
        let (on_other, on_ours, eligible) = self.tally(peers, now);
        if on_other >= PARTITION_MIN_OUTBOUND_ON_OTHER && on_other * 2 > eligible {
            PartitionVerdict::Hold { refusal, on_other, on_ours, eligible }
        } else {
            PartitionVerdict::Clear(ClearReason::PeersNotOnTheOtherChain { on_other, on_ours, eligible })
        }
    }

    /// The verified resync's confirmation: the synced chain refuses nothing heavier, at least
    /// [`RESYNC_MIN_CONFIRMING_PEERS`] long-lived outbound peers relayed blocks extending it within the
    /// window, and they are a strict majority of all eligible outbound peers.
    pub fn resync_verdict(&self, refusal: Option<PalwPartitionRefusalV1>, peers: &[PeerView], now: Instant) -> ResyncVerdict {
        let (on_other, on_ours, eligible) = self.tally(peers, now);
        let refusing = refusal.is_some();
        if !refusing && on_ours >= RESYNC_MIN_CONFIRMING_PEERS && on_ours * 2 > eligible {
            ResyncVerdict::Confirmed { on_ours, eligible }
        } else {
            ResyncVerdict::Pending { on_ours, on_other, eligible, refusing }
        }
    }

    /// `(on_other, on_ours, eligible)` over the long-lived outbound peers.
    fn tally(&self, peers: &[PeerView], now: Instant) -> (usize, usize, usize) {
        let recent = |t: Option<Instant>| t.is_some_and(|t| now.saturating_duration_since(t) <= PARTITION_OBSERVATION_WINDOW);
        let (mut eligible, mut on_other, mut on_ours) = (0usize, 0usize, 0usize);
        for p in peers.iter().filter(|p| p.outbound && p.connected_for >= PARTITION_MIN_PEER_AGE) {
            eligible += 1;
            let o = self.peers.get(&p.key).copied().unwrap_or_default();
            if recent(o.last_ours) {
                on_ours += 1;
            } else if recent(o.last_other) {
                on_other += 1;
            }
        }
        (on_other, on_ours, eligible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlockHash;
    use kaspa_utils::networking::{IpAddress, PeerId};
    use std::net::IpAddr;
    use uuid::Uuid;

    fn key(n: u16) -> PeerKey {
        PeerKey::new(PeerId::new(Uuid::from_u128(n as u128)), IpAddress::new(IpAddr::from([10, 1, (n >> 8) as u8, n as u8])))
    }

    fn view(n: u16, outbound: bool, minutes: u64) -> PeerView {
        PeerView { key: key(n), outbound, connected_for: Duration::from_secs(minutes * 60) }
    }

    /// B's record after the devnet partition: the majority's tip refused on every resolve while it
    /// advanced `span` ticks.
    fn refusal(span: u64) -> Option<PalwPartitionRefusalV1> {
        Some(PalwPartitionRefusalV1 {
            sink: BlockHash::from_u64_word(1),
            refused: BlockHash::from_u64_word(2),
            own_daa: 88,
            refused_daa: 87 + span,
            first_refused_daa: 87,
            resolves: 40,
        })
    }

    /// **A genuine minority after a partition holds.** Eight long-lived outbound peers, as B had: six
    /// on the majority relay only majority blocks; one minority peer (D6) relays this branch; one is
    /// silent. Six of eight are on the other chain — hold. And it releases on its own: when the refusal
    /// run ends (the sink moved, or nothing heavier is refused) and when the peers come back.
    #[test]
    fn a_genuine_minority_holds_and_releases_on_its_own() {
        let now = Instant::now();
        let mut w = PartitionWatch::default();
        let peers: Vec<_> = (0..8).map(|n| view(n, true, 60)).collect();
        for n in 0..6 {
            w.note_relay(key(n), RelayedRelation::Other, now);
        }
        w.note_relay(key(6), RelayedRelation::Other, now);
        w.note_relay(key(6), RelayedRelation::ExtendsOurs, now);
        assert_eq!(
            w.verdict(refusal(12), &peers, now),
            PartitionVerdict::Hold { refusal: refusal(12).unwrap(), on_other: 6, on_ours: 1, eligible: 8 }
        );
        assert_eq!(w.verdict(None, &peers, now), PartitionVerdict::Clear(ClearReason::NoRefusal), "healed: released");
        assert!(matches!(w.verdict(refusal(3), &peers, now), PartitionVerdict::Clear(ClearReason::RefusalTooShort { span_daa: 3 })));
        // The majority peers go quiet past the window: no longer evidence.
        let later = now + PARTITION_OBSERVATION_WINDOW + Duration::from_secs(1);
        assert!(matches!(w.verdict(refusal(12), &peers, later), PartitionVerdict::Clear(_)), "stale observations release it");
    }

    /// **The Sybil heavy-junk flood does NOT hold an honest node.** An attacker releases a heavier junk
    /// branch (forged attempt work, so the consensus half records a long refusal run) and opens two
    /// hundred inbound connections that relay only the junk. The node's eight long-lived outbound peers
    /// are honest: they relay the junk too (it is valid, so they hold and pass it on) but they also relay
    /// this chain's new blocks, so each counts as on OUR chain. Inbound peers are not counted at all.
    #[test]
    fn a_sybil_flood_relaying_a_heavy_junk_branch_does_not_hold() {
        let now = Instant::now();
        let mut w = PartitionWatch::default();
        let mut peers: Vec<_> = (0..8).map(|n| view(n, true, 120)).collect();
        for n in 0..8 {
            w.note_relay(key(n), RelayedRelation::Other, now);
            w.note_relay(key(n), RelayedRelation::ExtendsOurs, now);
        }
        for n in 100..300 {
            peers.push(view(n, false, 120));
            w.note_relay(key(n), RelayedRelation::Other, now);
        }
        assert_eq!(
            w.verdict(refusal(500), &peers, now),
            PartitionVerdict::Clear(ClearReason::PeersNotOnTheOtherChain { on_other: 0, on_ours: 8, eligible: 8 }),
            "two hundred inbound Sybils weigh nothing; the honest outbound peers are on our chain"
        );
    }

    /// Fresh outbound connections do not count either: an attacker that gets dialled (an address it
    /// planted) is not long-lived, and a node that just restarted holds nothing until its peers are.
    #[test]
    fn young_outbound_peers_do_not_count() {
        let now = Instant::now();
        let mut w = PartitionWatch::default();
        let mut peers: Vec<_> = (0..3).map(|n| view(n, true, 60)).collect();
        for n in 0..3 {
            w.note_relay(key(n), RelayedRelation::ExtendsOurs, now);
        }
        for n in 10..20 {
            peers.push(view(n, true, 2));
            w.note_relay(key(n), RelayedRelation::Other, now);
        }
        assert!(matches!(
            w.verdict(refusal(50), &peers, now),
            PartitionVerdict::Clear(ClearReason::PeersNotOnTheOtherChain { on_other: 0, on_ours: 3, eligible: 3 })
        ));
    }

    /// **The verified resync confirms only a chain the node's long-lived outbound peers are on.** A node
    /// that resynced onto the network's chain is confirmed once two of its three long-lived outbound peers
    /// relay blocks extending it. A node that synced an attacker's branch (its IBD peer lied) is NOT: its
    /// honest outbound peers relay another chain, and a Sybil flood of inbound peers relaying the
    /// attacker's chain counts for nothing. Nor is a chain that still refuses a heavier validated one.
    #[test]
    fn a_resync_is_confirmed_only_by_long_lived_outbound_peers_on_its_chain() {
        let now = Instant::now();
        let mut w = PartitionWatch::default();
        let mut peers: Vec<_> = (0..3).map(|n| view(n, true, 30)).collect();
        assert!(matches!(w.resync_verdict(None, &peers, now), ResyncVerdict::Pending { on_ours: 0, .. }), "nothing heard yet");
        w.note_relay(key(0), RelayedRelation::ExtendsOurs, now);
        w.note_relay(key(1), RelayedRelation::ExtendsOurs, now);
        assert_eq!(w.resync_verdict(None, &peers, now), ResyncVerdict::Confirmed { on_ours: 2, eligible: 3 });
        assert!(
            matches!(w.resync_verdict(refusal(20), &peers, now), ResyncVerdict::Pending { refusing: true, .. }),
            "a synced chain that refuses a heavier validated one is not confirmed"
        );

        // The attacker's branch: the node's own outbound peers are on another chain; two hundred inbound
        // Sybils relay the attacker's.
        let mut attacked = PartitionWatch::default();
        for n in 0..3 {
            attacked.note_relay(key(n), RelayedRelation::Other, now);
        }
        for n in 100..300 {
            peers.push(view(n, false, 30));
            attacked.note_relay(key(n), RelayedRelation::ExtendsOurs, now);
        }
        assert_eq!(
            attacked.resync_verdict(None, &peers, now),
            ResyncVerdict::Pending { on_ours: 0, on_other: 3, eligible: 3, refusing: false },
            "held: inbound peers confirm nothing"
        );
        // And fresh outbound connections confirm nothing either.
        let young: Vec<_> = (0..3).map(|n| view(n, true, 1)).collect();
        assert!(matches!(w.resync_verdict(None, &young, now), ResyncVerdict::Pending { eligible: 0, .. }));
    }

    /// One outbound peer is never enough, and a silent majority of peers holds nothing back.
    #[test]
    fn one_peer_or_a_silent_majority_is_not_enough() {
        let now = Instant::now();
        let mut w = PartitionWatch::default();
        let peers = vec![view(0, true, 60)];
        w.note_relay(key(0), RelayedRelation::Other, now);
        assert!(matches!(w.verdict(refusal(50), &peers, now), PartitionVerdict::Clear(_)), "a single outbound peer");
        let peers: Vec<_> = (0..6).map(|n| view(n, true, 60)).collect();
        w.note_relay(key(1), RelayedRelation::Other, now);
        assert_eq!(
            w.verdict(refusal(50), &peers, now),
            PartitionVerdict::Clear(ClearReason::PeersNotOnTheOtherChain { on_other: 2, on_ours: 0, eligible: 6 }),
            "two of six, four silent: not a majority"
        );
        w.forget_peer(&key(0));
        assert!(matches!(w.verdict(refusal(50), &peers, now), PartitionVerdict::Clear(_)));
    }
}
