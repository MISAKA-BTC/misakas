//! PALW claim-material and seat-receipt gossip (ADR-0042 Decision 7's transport half).
//!
//! **What this closes.** The consensus side of the claim lattice was complete — `palw_seat_duties_v2`
//! tells a node which claims it is seated on, `base0_material_matches_claim_v1` decides what a seat
//! should answer, and a signed quorum licenses a claim — but the producer wrote its retained
//! execution to its own disk and *nothing served it and nothing fetched it*. A seat on another host
//! had no way to obtain the tiles, so no honest panel could file a `ReceiptLicensed` on a real
//! network however the service was written (launch blockers: "what is still missing").
//!
//! **Why broadcast and not request/response.** The material is small — the RC floor's measured
//! encoding is 2.27 MB, once per 120 s block — and data availability *wants* the bytes public: one
//! flood serves all five seats, needs no reply routing through relaying peers, and doubles as the
//! producer discharging its retention obligation in the open. The bytes authenticate themselves:
//! a seat verifies them against the claim's own committed roots, so the transport carries no
//! signatures and trusts no peer.
//!
//! **Flood control, and what an attacker can do.** Every message is admitted at most once per
//! digest (relay-once), materials are capped at [`PALW_MATERIAL_MAX_BYTES`] and further capped at
//! [`PALW_MATERIALS_PER_CLAIM`] distinct payloads per claim — an attacker who knows a live claim id
//! can make each node relay at most that many garbage blobs for it, bounded, and the honest
//! producer's re-broadcast still fits because distinct bytes have distinct digests. Receipts are
//! ~5 KB and capped by digest alone. The dedup key is a process-seeded SipHash: not collision-proof,
//! but a collision only suppresses a RELAY, never a verdict, and the seed is not the attacker's to
//! predict.
//!
//! Nothing here is consensus: a node that drops every message merely fails to hear receipts.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::sync::Mutex;

use kaspa_hashes::Hash64;
use tokio::sync::mpsc;

/// Hard cap on one material broadcast.
///
/// Sized from the LARGEST registered class, not the floor: QWEN25-A16's canonical job encodes to
/// 9.7 MB and QWEN36's to 8.9 MB, and the previous 8 MiB cap — "~3.5× headroom" over the floor's
/// 2.27 MB — silently dropped both. The failure it caused was not a bigger memory bill but a
/// SLASHING machine: no seat ever received an LLM claim's material, every panel concluded
/// `Unavailable` at the half-window, and every honest LLM producer was defaulted and slashed by
/// quorum, block after block. A transport cap must never be able to overrule an admission the
/// consensus already accepted — so it is sized ~1.7× over the largest class this build ships.
pub const PALW_MATERIAL_MAX_BYTES: usize = 16 << 20;
/// Hard cap on one receipt broadcast — an ML-DSA-87 signature is 4,627 bytes and the rest of the
/// receipt is under a hundred.
pub const PALW_RECEIPT_MAX_BYTES: usize = 16 << 10;
/// Distinct material payloads admitted per claim. The honest producer needs one; the rest is an
/// attacker's budget.
pub const PALW_MATERIALS_PER_CLAIM: usize = 4;
/// How many digests the relay-once memory holds before the oldest is forgotten. Forgetting an old
/// digest re-admits an old message once — a cost, not a fault.
const SEEN_CAP: usize = 4096;
/// The inbox bound. A consumer that stalls loses the OLDEST unread events; a node with no PALW
/// role never takes the receiver and the channel simply fills and drops.
const INBOX_CAP: usize = 256;

/// What the gossip flow feeds into the node's PALW services.
#[derive(Debug)]
pub enum PalwGossipEvent {
    /// A claim's execution material, as broadcast. Unverified bytes — the consumer runs the
    /// material check; this layer only bounded and deduplicated them.
    Material { claim: Hash64, bytes: Vec<u8> },
    /// One seat's signed receipt, as broadcast. Unverified bytes — the quorum validator is the
    /// authority on whether they mean anything.
    Receipt { bytes: Vec<u8> },
}

/// The verdict [`PalwGossipCenter::admit_material`]/[`admit_receipt`](PalwGossipCenter::admit_receipt)
/// hand the flow: relay it on, or say precisely why not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGossipAdmit {
    /// First sighting, within every cap: forward to the inbox and relay to other peers.
    Fresh,
    /// Seen before (or the claim's material budget is spent): do nothing, silently.
    Duplicate,
    /// Over the byte cap: drop without decoding. Not a protocol error — a future class may
    /// honestly outgrow our cap, and a bigger-material network is one this binary just does not
    /// relay for.
    TooBig,
}

pub struct PalwGossipCenter {
    state: Mutex<GossipState>,
    inbox_tx: mpsc::Sender<PalwGossipEvent>,
    inbox_rx: Mutex<Option<mpsc::Receiver<PalwGossipEvent>>>,
    /// **Is anybody reading?** Set once, when the node's PALW service takes the receiver.
    ///
    /// Most nodes on a PALW network run no panel and no producer, so most nodes never take it —
    /// and an untaken `mpsc` channel is not a no-op, it is a buffer. Every relayed material was
    /// copied into it and held for the life of the process: `INBOX_CAP` * `PALW_MATERIAL_MAX_BYTES`
    /// is 2 GiB of memory a node accrues for a role it does not have, filled by peers, with no
    /// bond and no block required. Checking this before the `to_vec` skips the copy as well as the
    /// queue, on the majority of nodes.
    inbox_taken: std::sync::atomic::AtomicBool,
    hasher: RandomState,
    /// **Where a served material comes from** — registered by the node's panel service, which is
    /// the party holding a retention directory (its own captures AND, since the pull transport,
    /// every foreign material it heard). `None` on the majority of nodes, which then simply never
    /// answer a request. The closure does disk I/O; callers hold no lock while invoking it.
    material_resolver: Mutex<Option<std::sync::Arc<dyn Fn(Hash64) -> Option<Vec<u8>> + Send + Sync>>>,
    /// Serve throttle: a claim answered within the last window is not answered again — one
    /// request refills every peer (the serve is a broadcast), so immediate repeats are pure
    /// amplification. Keyed by claim, pruned inline.
    served_recently: Mutex<HashMap<Hash64, std::time::Instant>>,
    /// What this node has ASKED for and not yet received, with the time it asked.
    ///
    /// A solicited answer must not be refused by the per-claim relay budget (audit M2-1): four
    /// ~70-byte payloads from a stranger spend that budget before the honest multi-megabyte
    /// material arrives, and then the very pull that exists to recover from it is dropped too.
    /// Bounded by count and by TTL, and only this node's own requests ever write it.
    outstanding_pulls: Mutex<HashMap<Hash64, std::time::Instant>>,
    /// How many bytes this node has emitted answering pulls in the current window.
    serve_budget: Mutex<ServeBudget>,
}

struct ServeBudget {
    window_started: std::time::Instant,
    bytes_served: u64,
}

/// The window and ceiling for answering pulls. 64 MiB a minute is ~8 of the largest registered
/// class's materials — enough to unstick a neighbourhood, far below what a request loop can
/// otherwise conjure (measured attack: 8 GiB enqueued from 4.5 KB of requests).
const SERVE_BUDGET_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
const SERVE_BUDGET_BYTES_PER_WINDOW: u64 = 64 << 20;
/// A pull's answer is exempt from the per-claim budget for this long after asking.
const PULL_SOLICITED_TTL: std::time::Duration = std::time::Duration::from_secs(120);
/// Never track more outstanding pulls than a panel could plausibly have open at once.
const OUTSTANDING_PULL_CAP: usize = 512;

struct GossipState {
    seen: HashSet<u64>,
    /// Each remembered digest, with the claim its per-claim count was charged to. The claim
    /// travels WITH the digest so that evicting one decrements the other — see
    /// `materials_per_claim`.
    seen_order: VecDeque<(u64, Option<Hash64>)>,
    /// Per-claim material counts, and **a derived index of `seen_order`, not a map of its own.**
    ///
    /// It used to gain an entry per claim id and never lose one. Claim ids are 64-byte values
    /// taken straight off the wire — no on-chain existence check, no signature, no bond binding,
    /// no rate limit — so a peer could name a fresh one per message and grow this map until the
    /// node died, without holding a bond or landing a single block. Keying eviction to the
    /// digest FIFO bounds it at `SEEN_CAP` claims by construction.
    materials_per_claim: HashMap<Hash64, usize>,
}

impl Default for PalwGossipCenter {
    fn default() -> Self {
        let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_CAP);
        Self {
            state: Mutex::new(GossipState { seen: HashSet::new(), seen_order: VecDeque::new(), materials_per_claim: HashMap::new() }),
            inbox_tx,
            inbox_rx: Mutex::new(Some(inbox_rx)),
            inbox_taken: std::sync::atomic::AtomicBool::new(false),
            hasher: RandomState::new(),
            material_resolver: Mutex::new(None),
            served_recently: Mutex::new(HashMap::new()),
            outstanding_pulls: Mutex::new(HashMap::new()),
            serve_budget: Mutex::new(ServeBudget { window_started: std::time::Instant::now(), bytes_served: 0 }),
        }
    }
}

impl PalwGossipCenter {
    /// The consuming end, taken exactly once by the node's PALW panel/producer service. A second
    /// taker gets `None` — two consumers would each see half the events.
    /// Register the closure a serve consults — panel service only; see the field's doc.
    pub fn set_material_resolver(&self, resolver: std::sync::Arc<dyn Fn(Hash64) -> Option<Vec<u8>> + Send + Sync>) {
        *self.material_resolver.lock().unwrap() = Some(resolver);
    }

    /// The material behind `claim`, if this node holds it and has not just served it.
    ///
    /// `None` is "stay silent" — no resolver registered (not a panel), nothing on disk, or served
    /// within the throttle window. The 10-second window is per claim: the serve is a broadcast,
    /// so one answer refills every peer at once and an immediate repeat can only be a replay or a
    /// flood.
    pub fn resolve_material_for_serve(&self, claim: Hash64) -> Option<Vec<u8>> {
        {
            let mut served = self.served_recently.lock().unwrap();
            let now = std::time::Instant::now();
            served.retain(|_, at| now.duration_since(*at) < std::time::Duration::from_secs(60));
            if let Some(at) = served.get(&claim)
                && now.duration_since(*at) < std::time::Duration::from_secs(10)
            {
                return None;
            }
        }
        // **The disk read happens with NO lock held.** The closure reads up to 16 MiB
        // synchronously; holding the global resolver mutex across it (as this did until audit
        // M2-2) serialises every serve on the slowest one and contradicts the field's own doc.
        let resolver = { self.material_resolver.lock().unwrap().clone() }?;
        let bytes = resolver(claim)?;
        if bytes.len() > PALW_MATERIAL_MAX_BYTES {
            return None; // never serve what the transport would refuse
        }
        // **A global budget, because the per-claim throttle is not one** (audit M2-2). The
        // requester picks the claim, so a per-claim window bounds repetition and nothing else: K
        // planted claim ids cycled past it produce K serves back to back. This caps what the node
        // will emit in answer to pulls at all, whatever mix of claims is asked for.
        {
            let mut budget = self.serve_budget.lock().unwrap();
            let now = std::time::Instant::now();
            if now.duration_since(budget.window_started) >= SERVE_BUDGET_WINDOW {
                budget.window_started = now;
                budget.bytes_served = 0;
            }
            if budget.bytes_served.saturating_add(bytes.len() as u64) > SERVE_BUDGET_BYTES_PER_WINDOW {
                return None;
            }
            budget.bytes_served += bytes.len() as u64;
        }
        self.served_recently.lock().unwrap().insert(claim, std::time::Instant::now());
        Some(bytes)
    }

    pub fn take_inbox(&self) -> Option<mpsc::Receiver<PalwGossipEvent>> {
        let taken = self.inbox_rx.lock().unwrap().take();
        if taken.is_some() {
            self.inbox_taken.store(true, std::sync::atomic::Ordering::Release);
        }
        taken
    }

    /// Whether anything will ever read what we queue. Relay and dedup do not depend on it — a node
    /// with no PALW role still carries the network's traffic; it just does not keep a copy.
    fn has_consumer(&self) -> bool {
        self.inbox_taken.load(std::sync::atomic::Ordering::Acquire)
    }

    fn digest(&self, kind: u8, claim: Option<&Hash64>, bytes: &[u8]) -> u64 {
        let mut h = self.hasher.build_hasher();
        kind.hash(&mut h);
        if let Some(claim) = claim {
            claim.as_byte_slice().hash(&mut h);
        }
        bytes.hash(&mut h);
        h.finish()
    }

    /// **Record that this node has asked the network for `claim`.**
    ///
    /// Called by the panel just before it emits a `PalwMaterialRequest`. For the next
    /// [`PULL_SOLICITED_TTL`] the answer is exempt from the per-claim relay budget — see
    /// [`Self::admit_material`]. Bounded by TTL and by [`OUTSTANDING_PULL_CAP`].
    pub fn note_pull_request(&self, claim: Hash64) {
        let mut pulls = self.outstanding_pulls.lock().unwrap();
        let now = std::time::Instant::now();
        pulls.retain(|_, at| now.duration_since(*at) < PULL_SOLICITED_TTL);
        if pulls.len() >= OUTSTANDING_PULL_CAP {
            return;
        }
        pulls.insert(claim, now);
    }

    /// Is an answer for `claim` something this node asked for and is still waiting on?
    fn is_solicited(&self, claim: Hash64) -> bool {
        let pulls = self.outstanding_pulls.lock().unwrap();
        pulls.get(&claim).is_some_and(|at| std::time::Instant::now().duration_since(*at) < PULL_SOLICITED_TTL)
    }

    fn admit_digest(&self, digest: u64, material_claim: Option<Hash64>, solicited: bool) -> PalwGossipAdmit {
        let mut state = self.state.lock().unwrap();
        if state.seen.contains(&digest) {
            return PalwGossipAdmit::Duplicate;
        }
        if let Some(claim) = material_claim {
            let count = state.materials_per_claim.entry(claim).or_insert(0);
            // **The budget does not apply to an answer this node asked for** (audit M2-1). The
            // counter is charged before anything knows who sent the bytes or whether they verify,
            // so four ~70-byte payloads from a stranger make the honest material `Duplicate`
            // network-wide — the seat then never sees it, signs `Unavailable`, and an honest
            // producer's bond is slashed for ~280 bytes of attacker traffic. A solicited answer is
            // still digest-deduplicated and still size-capped; it just cannot be crowded out.
            if *count >= PALW_MATERIALS_PER_CLAIM && !solicited {
                return PalwGossipAdmit::Duplicate;
            }
            *count += 1;
        }
        state.seen.insert(digest);
        state.seen_order.push_back((digest, material_claim));
        if state.seen_order.len() > SEEN_CAP
            && let Some((old, claim)) = state.seen_order.pop_front()
        {
            state.seen.remove(&old);
            // Give the count back with the digest that took it, so the map cannot outlive the
            // FIFO that feeds it.
            if let Some(claim) = claim
                && let Some(count) = state.materials_per_claim.get_mut(&claim)
            {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    state.materials_per_claim.remove(&claim);
                }
            }
        }
        PalwGossipAdmit::Fresh
    }

    /// Admit a material broadcast. `Fresh` means: push [`PalwGossipEvent::Material`] to the inbox
    /// (done here) and the caller should relay the message to its other peers.
    pub fn admit_material(&self, claim: Hash64, bytes: &[u8]) -> PalwGossipAdmit {
        if bytes.len() > PALW_MATERIAL_MAX_BYTES {
            return PalwGossipAdmit::TooBig;
        }
        let verdict = self.admit_digest(self.digest(1, Some(&claim), bytes), Some(claim), self.is_solicited(claim));
        // Drop-on-full: a node with no consumer is not obliged to remember.
        if verdict == PalwGossipAdmit::Fresh && self.has_consumer() {
            let _ = self.inbox_tx.try_send(PalwGossipEvent::Material { claim, bytes: bytes.to_vec() });
        }
        verdict
    }

    /// Admit a receipt broadcast. Same contract as [`Self::admit_material`].
    pub fn admit_receipt(&self, bytes: &[u8]) -> PalwGossipAdmit {
        if bytes.len() > PALW_RECEIPT_MAX_BYTES {
            return PalwGossipAdmit::TooBig;
        }
        let verdict = self.admit_digest(self.digest(2, None, bytes), None, false);
        if verdict == PalwGossipAdmit::Fresh && self.has_consumer() {
            let _ = self.inbox_tx.try_send(PalwGossipEvent::Receipt { bytes: bytes.to_vec() });
        }
        verdict
    }

    /// Mark this node's OWN outgoing message seen, so the echo a peer relays back is a duplicate
    /// rather than a second inbox event.
    /// Mark our own material seen so the echo is not re-admitted — AND hand it to our own inbox.
    ///
    /// The second half is what lets a producer answer a court about its own claim. A dispute is
    /// opened against the executor, and the only party that can disclose the execution's state at
    /// a rung is the party that ran it; a producer whose own capture never reached its own panel
    /// service had nothing to answer with, and the session ran out. Everyone else gets these bytes
    /// over the wire, so the executor being the one node without them was the wrong asymmetry.
    pub fn mark_own_material(&self, claim: Hash64, bytes: &[u8]) {
        let _ = self.admit_digest(self.digest(1, Some(&claim), bytes), Some(claim), true);
        if self.has_consumer() {
            let _ = self.inbox_tx.try_send(PalwGossipEvent::Material { claim, bytes: bytes.to_vec() });
        }
    }

    pub fn mark_own_receipt(&self, bytes: &[u8]) {
        let _ = self.admit_digest(self.digest(2, None, bytes), None, true);
    }
}

#[cfg(test)]
mod tests {

    /// **A node with no PALW role keeps no copies.**
    ///
    /// The inbox is an `mpsc` channel, and an untaken channel is a buffer, not a no-op: every
    /// relayed material was copied into it and held for the life of the process. `INBOX_CAP` *
    /// `PALW_MATERIAL_MAX_BYTES` is 2 GiB that peers could make a bystander hold, with no bond and
    /// no block. Relay and dedup must be unaffected — a node without the role still carries the
    /// network's traffic.
    #[test]
    fn a_node_with_no_consumer_relays_without_buffering() {
        let center = PalwGossipCenter::default();
        let claim = Hash64::from_u64_word(7);
        assert_eq!(center.admit_material(claim, b"first"), PalwGossipAdmit::Fresh, "relay is not gated on having a consumer");
        assert_eq!(center.admit_material(claim, b"first"), PalwGossipAdmit::Duplicate, "and dedup still works");
        assert!(!center.has_consumer());

        // Nothing was queued, so the receiver a late-starting service takes is empty rather than
        // holding what arrived while nobody was reading.
        let mut rx = center.take_inbox().expect("the first taker gets it");
        assert!(rx.try_recv().is_err(), "a bystander must not have accumulated payloads");

        // ...and once somebody IS reading, the same call queues.
        assert_eq!(center.admit_material(Hash64::from_u64_word(8), b"second"), PalwGossipAdmit::Fresh);
        assert!(rx.try_recv().is_ok(), "with a consumer present the event is delivered");
    }
    use super::*;

    /// **The per-claim map grew forever, and nothing on the wire was authenticated enough to stop
    /// it.** Claim ids arrive as 64 raw bytes with no on-chain existence check, no signature and
    /// no bond binding, so a peer could name a fresh claim per message and grow this map until the
    /// node died — while holding no bond and landing no block. Bounding it by the digest FIFO
    /// makes it a derived index instead of an independent map.
    #[test]
    fn the_material_map_cannot_outgrow_the_digest_window() {
        let center = PalwGossipCenter::default();
        for n in 0..(SEEN_CAP as u64 * 3) {
            let claim = Hash64::from_u64_word(n);
            let digest = center.digest(1, Some(&claim), &n.to_le_bytes());
            let _ = center.admit_digest(digest, Some(claim), false);
        }
        let state = center.state.lock().unwrap();
        assert!(
            state.materials_per_claim.len() <= SEEN_CAP,
            "the map held {} claims against a {SEEN_CAP}-digest window",
            state.materials_per_claim.len()
        );
        assert_eq!(state.seen.len(), state.seen_order.len(), "the two halves of the window agree");
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// Relay-once, per-claim material budget, size caps, and the own-echo suppression — the whole
    /// flood-control contract in one place.
    #[test]
    fn the_gossip_center_bounds_what_it_relays() {
        let center = PalwGossipCenter::default();
        // This test measures what reaches the INBOX, so it has to be a node that reads one. The
        // receiver is taken up front rather than at the end because queuing is now conditional on
        // a consumer existing (see `a_node_with_no_consumer_relays_without_buffering`) — taking it
        // afterwards measured a bystander and would report zero.
        let mut rx = center.take_inbox().expect("first taker");
        assert!(center.take_inbox().is_none(), "one consumer, once");
        let claim = h64(7);

        assert_eq!(center.admit_material(claim, b"honest"), PalwGossipAdmit::Fresh);
        assert_eq!(center.admit_material(claim, b"honest"), PalwGossipAdmit::Duplicate, "relay-once");
        assert_eq!(center.admit_material(claim, b"garbage-1"), PalwGossipAdmit::Fresh);
        assert_eq!(center.admit_material(claim, b"garbage-2"), PalwGossipAdmit::Fresh);
        assert_eq!(center.admit_material(claim, b"garbage-3"), PalwGossipAdmit::Fresh);
        assert_eq!(
            center.admit_material(claim, b"garbage-4"),
            PalwGossipAdmit::Duplicate,
            "the per-claim budget is spent — an attacker cannot make a node relay unboundedly for one claim"
        );
        assert_eq!(center.admit_material(h64(8), b"other-claim"), PalwGossipAdmit::Fresh, "another claim has its own budget");

        assert_eq!(center.admit_material(h64(9), &vec![0u8; PALW_MATERIAL_MAX_BYTES + 1]), PalwGossipAdmit::TooBig);
        assert_eq!(center.admit_receipt(&vec![0u8; PALW_RECEIPT_MAX_BYTES + 1]), PalwGossipAdmit::TooBig);

        assert_eq!(center.admit_receipt(b"receipt"), PalwGossipAdmit::Fresh);
        assert_eq!(center.admit_receipt(b"receipt"), PalwGossipAdmit::Duplicate);

        center.mark_own_receipt(b"mine");
        assert_eq!(center.admit_receipt(b"mine"), PalwGossipAdmit::Duplicate, "our own echo does not come back as an event");

        // And the inbox saw exactly the fresh ones that were not our own mark.
        let mut materials = 0;
        let mut receipts = 0;
        while let Ok(event) = rx.try_recv() {
            match event {
                PalwGossipEvent::Material { .. } => materials += 1,
                PalwGossipEvent::Receipt { .. } => receipts += 1,
            }
        }
        assert_eq!((materials, receipts), (5, 1), "4 for claim 7, 1 for claim 8; the one receipt — marks push nothing");
    }
}
