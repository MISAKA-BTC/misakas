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
use kaspa_p2p_lib::PeerKey;
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
    /// **Per requesting peer** (audit3 H5/H10). A budget keyed on nothing is a budget one peer
    /// spends: at the shipped 9.7 MB material the node-wide 64 MiB/60 s allowed 6.9 serves per
    /// MINUTE for the whole node, so seven ~80-byte requests a minute — 560 bytes — silenced every
    /// honest seat's pull for the remaining 59 seconds of every window. The seats then cannot
    /// obtain material, sign `Unavailable`, and three of five slash the honest producer's bond.
    /// Cleared with the window; peers are bounded by the connection count.
    per_peer: HashMap<PeerKey, u64>,
}

/// The window and ceilings for answering pulls.
///
/// **Two of them, because one is not a bound** (audit3 H5/H10). The node-wide figure alone was
/// spendable by any single peer, and it was also too small to be honest: five seats pulling one
/// 9.7 MB claim consume 48.5 MB, so two claims pulled in the same minute exceeded the node's whole
/// allowance with no attacker present at all. The per-peer share is what actually bounds an
/// attacker — it controls its own connections, not the node's other peers — and the node-wide
/// figure is the operator's egress backstop behind it, sized so an ordinary fan-out fits.
const SERVE_BUDGET_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
const SERVE_BUDGET_BYTES_PER_WINDOW: u64 = 256 << 20;
/// One peer's share: three of the largest registered class's materials per minute. A seat re-pulls
/// on a 25-DAA throttle (~50 minutes at the frozen cadence), so this is far above any honest need
/// and far below what a request loop can conjure.
const SERVE_BUDGET_BYTES_PER_PEER: u64 = 48 << 20;
/// How long one claim stays un-servable after a serve is ATTEMPTED for it.
const SERVE_THROTTLE: std::time::Duration = std::time::Duration::from_secs(10);
/// A pull's answer is exempt from the per-claim budget for this long after asking.
const PULL_SOLICITED_TTL: std::time::Duration = std::time::Duration::from_secs(120);
/// Never track more outstanding pulls than a panel could plausibly have open at once.
const OUTSTANDING_PULL_CAP: usize = 512;

/// **The serve throttle is a courtesy, and it must not become the cost centre.**
///
/// `served_recently` is keyed by a claim id taken straight off the wire — no on-chain existence
/// check, no signature, no per-peer request rate — and the window sweep used to walk the WHOLE map
/// on every request, under a `std::sync::Mutex` held from an async task. A peer sending a fresh
/// random claim id per ~70-byte request therefore bought one entry and one full walk each time:
/// unbounded memory and quadratic CPU inside a lock every runtime worker can block on. The sibling
/// map `outstanding_pulls`, which only this node writes, was already capped; the peer-written one
/// was not.
const SERVED_RECENTLY_CAP: usize = 4096;
/// **A ceiling on the solicited exemption, and a floor of slots one peer cannot take** (audit3 H7).
///
/// The exemption existed so four cheap payloads from a stranger could not make the honest answer
/// `Duplicate` network-wide. But it was a total bypass: for the 120 s TTL an UNBOUNDED number of
/// distinct payloads for that claim were `Fresh`, each relayed to every peer and copied into the
/// inbox, and the panel pool evicts its oldest — so "whoever is first must not lock out whoever is
/// right" became "whoever is LAST decides", which the attacker always is. Two bounds now: the
/// exemption has a ceiling, and no single peer may take more than a couple of a claim's slots, so
/// there is always room for somebody else's answer.
/// **Residual, stated rather than hidden.** These bound what ONE peer may occupy. An attacker with
/// several distinct peer keys — several IPs — still gets its couple of slots per key, so the
/// per-claim ceiling is what stops it there; the connection manager's peer limit is the outer
/// bound. What is closed is the cheap version, where one connection sending continuously was always
/// the LAST voice and the panel pool evicts its oldest.
const PALW_SOLICITED_MATERIALS_PER_CLAIM: usize = 16;
const PALW_MATERIALS_PER_PEER_PER_CLAIM: usize = 2;

struct GossipState {
    seen: HashSet<u64>,
    /// Each remembered digest, with the claim AND the peer its counts were charged to. Both travel
    /// WITH the digest so that evicting one decrements the others — see `materials_per_claim` and
    /// `materials_per_peer_claim`.
    seen_order: VecDeque<(u64, Option<(PeerKey, Hash64)>)>,
    /// Per-claim material counts, and **a derived index of `seen_order`, not a map of its own.**
    ///
    /// It used to gain an entry per claim id and never lose one. Claim ids are 64-byte values
    /// taken straight off the wire — no on-chain existence check, no signature, no bond binding,
    /// no rate limit — so a peer could name a fresh one per message and grow this map until the
    /// node died, without holding a bond or landing a single block. Keying eviction to the
    /// digest FIFO bounds it at `SEEN_CAP` claims by construction.
    materials_per_claim: HashMap<Hash64, usize>,
    /// How many of a claim's admitted payloads came from one peer (audit3 H7). Same FIFO-derived
    /// lifetime as `materials_per_claim`, so it cannot outlive the digests that fed it.
    materials_per_peer_claim: HashMap<(PeerKey, Hash64), usize>,
}

impl Default for PalwGossipCenter {
    fn default() -> Self {
        let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_CAP);
        Self {
            state: Mutex::new(GossipState {
                seen: HashSet::new(),
                seen_order: VecDeque::new(),
                materials_per_claim: HashMap::new(),
                materials_per_peer_claim: HashMap::new(),
            }),
            inbox_tx,
            inbox_rx: Mutex::new(Some(inbox_rx)),
            inbox_taken: std::sync::atomic::AtomicBool::new(false),
            hasher: RandomState::new(),
            material_resolver: Mutex::new(None),
            served_recently: Mutex::new(HashMap::new()),
            outstanding_pulls: Mutex::new(HashMap::new()),
            serve_budget: Mutex::new(ServeBudget {
                window_started: std::time::Instant::now(),
                bytes_served: 0,
                per_peer: HashMap::new(),
            }),
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

    /// Reserve `bytes` of this window's serve allowance for `peer`. `false` means refuse.
    ///
    /// Reserved BEFORE the disk read and refunded afterwards (audit3 H6): the read is the expensive
    /// step and the budget used to sit behind it, so the ceiling bounded egress while bounding
    /// neither disk I/O nor the worker the read blocks.
    fn reserve_serve_budget(&self, peer: PeerKey, bytes: u64) -> bool {
        let mut budget = self.serve_budget.lock().unwrap();
        let now = std::time::Instant::now();
        if now.duration_since(budget.window_started) >= SERVE_BUDGET_WINDOW {
            budget.window_started = now;
            budget.bytes_served = 0;
            budget.per_peer.clear();
        }
        let peer_used = budget.per_peer.get(&peer).copied().unwrap_or(0);
        if peer_used.saturating_add(bytes) > SERVE_BUDGET_BYTES_PER_PEER {
            return false;
        }
        if budget.bytes_served.saturating_add(bytes) > SERVE_BUDGET_BYTES_PER_WINDOW {
            return false;
        }
        budget.per_peer.insert(peer, peer_used + bytes);
        budget.bytes_served += bytes;
        true
    }

    /// Give back the part of a reservation that was not sent.
    fn refund_serve_budget(&self, peer: PeerKey, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let mut budget = self.serve_budget.lock().unwrap();
        budget.bytes_served = budget.bytes_served.saturating_sub(bytes);
        if let Some(used) = budget.per_peer.get_mut(&peer) {
            *used = used.saturating_sub(bytes);
        }
    }

    /// The material behind `claim`, if this node holds it, has allowance for `peer`, and has not
    /// just been asked for it.
    ///
    /// `None` is "stay silent" — no resolver registered (not a panel), nothing on disk, out of
    /// allowance, or asked for within the throttle window.
    ///
    /// **The order of the three gates is the whole point** (audit3 H6). It used to be: throttle,
    /// read, size, budget, record-the-throttle. So the cheapest gate ran last and the most
    /// expensive step ran second — an 80-byte request bought a full synchronous `std::fs::read` of
    /// up to 16 MiB even when the budget was spent and nothing would be emitted. Worse, the
    /// throttle was recorded only on SUCCESS, so a request the budget refused was never marked and
    /// the identical request could be repeated immediately and forever, with zero outbound traffic
    /// to make it visible. Now the throttle is charged on the ATTEMPT, the allowance is reserved
    /// before the read, and the read itself runs on a blocking thread instead of the shared
    /// runtime that also carries block relay, IBD and RPC — which is what audit M2-2 prescribed
    /// and did not land.
    pub async fn resolve_material_for_serve(&self, peer: PeerKey, claim: Hash64) -> Option<Vec<u8>> {
        {
            let mut served = self.served_recently.lock().unwrap();
            let now = std::time::Instant::now();
            // Amortised, and bounded: sweep only when the map is at its cap, and if the sweep does
            // not get it under the cap — every entry still inside its window — drop the whole map.
            // Losing the throttle for a moment lets a claim be served sooner than it might have
            // been; keeping an unbounded map lets a stranger decide how much memory this node uses
            // and how long every other peer waits for the lock.
            if served.len() >= SERVED_RECENTLY_CAP {
                served.retain(|_, at| now.duration_since(*at) < std::time::Duration::from_secs(60));
                if served.len() >= SERVED_RECENTLY_CAP {
                    served.clear();
                }
            }
            if let Some(at) = served.get(&claim)
                && now.duration_since(*at) < SERVE_THROTTLE
            {
                return None;
            }
            // Charged here, on the attempt. A refusal below must cost the asker its window too, or
            // the refusal is free and repeatable.
            served.insert(claim, now);
        }
        // Reserve the worst case; the difference comes back once the real size is known.
        let reservation = PALW_MATERIAL_MAX_BYTES as u64;
        if !self.reserve_serve_budget(peer, reservation) {
            return None;
        }
        // **The disk read happens with NO lock held and NOT on an async worker.** The closure reads
        // up to 16 MiB synchronously; holding the global resolver mutex across it (as this did
        // until audit M2-2) serialises every serve on the slowest one, and running it inline on the
        // shared tokio runtime (as it did until audit3 H6) lets an 80-byte request pin a reactor
        // thread.
        let resolver = { self.material_resolver.lock().unwrap().clone() };
        let Some(resolver) = resolver else {
            self.refund_serve_budget(peer, reservation);
            return None;
        };
        let read = tokio::task::spawn_blocking(move || resolver(claim)).await;
        let Some(bytes) = read.ok().flatten() else {
            self.refund_serve_budget(peer, reservation);
            return None;
        };
        if bytes.len() > PALW_MATERIAL_MAX_BYTES {
            self.refund_serve_budget(peer, reservation); // never serve what the transport would refuse
            return None;
        }
        self.refund_serve_budget(peer, reservation.saturating_sub(bytes.len() as u64));
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

    /// Undo an [`Self::admit_digest`] whose payload never reached the consumer, giving back the
    /// per-claim and per-peer slots with it so neither map outlives the FIFO that feeds it — the
    /// same bookkeeping the FIFO's own eviction does, in the other direction.
    fn forget_digest(&self, digest: u64) {
        let mut state = self.state.lock().unwrap();
        if !state.seen.remove(&digest) {
            return;
        }
        let Some(position) = state.seen_order.iter().position(|(seen, _)| *seen == digest) else { return };
        let Some((_, material)) = state.seen_order.remove(position) else { return };
        let Some((peer, claim)) = material else { return };
        if let Some(count) = state.materials_per_claim.get_mut(&claim) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.materials_per_claim.remove(&claim);
            }
        }
        if let Some(count) = state.materials_per_peer_claim.get_mut(&(peer, claim)) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.materials_per_peer_claim.remove(&(peer, claim));
            }
        }
    }

    fn admit_digest(&self, digest: u64, material: Option<(PeerKey, Hash64)>, solicited: bool) -> PalwGossipAdmit {
        let mut state = self.state.lock().unwrap();
        if state.seen.contains(&digest) {
            return PalwGossipAdmit::Duplicate;
        }
        if let Some((peer, claim)) = material {
            // **No single peer may take more than a couple of a claim's slots** (audit3 H7). This
            // is the bound that keeps room for the honest answer: with four slots and a per-peer
            // ceiling of two, a flooder cannot be the only voice for a claim however fast it sends.
            let from_peer = state.materials_per_peer_claim.get(&(peer, claim)).copied().unwrap_or(0);
            if from_peer >= PALW_MATERIALS_PER_PEER_PER_CLAIM {
                return PalwGossipAdmit::Duplicate;
            }
            let count = state.materials_per_claim.entry(claim).or_insert(0);
            // **The budget does not apply to an answer this node asked for** (audit M2-1). The
            // counter is charged before anything knows who sent the bytes or whether they verify,
            // so four ~70-byte payloads from a stranger make the honest material `Duplicate`
            // network-wide — the seat then never sees it, signs `Unavailable`, and an honest
            // producer's bond is slashed for ~280 bytes of attacker traffic. A solicited answer is
            // still digest-deduplicated and still size-capped; it just cannot be crowded out.
            //
            // **But the exemption has a ceiling** (audit3 H7). Unbounded, it meant that for the
            // 120 s TTL every distinct payload for that claim was relayed to every peer — an
            // amplifier switched on by the very pull that exists to recover from a flood.
            let cap = if solicited { PALW_SOLICITED_MATERIALS_PER_CLAIM } else { PALW_MATERIALS_PER_CLAIM };
            if *count >= cap {
                return PalwGossipAdmit::Duplicate;
            }
            *count += 1;
            *state.materials_per_peer_claim.entry((peer, claim)).or_insert(0) += 1;
        }
        state.seen.insert(digest);
        state.seen_order.push_back((digest, material));
        if state.seen_order.len() > SEEN_CAP
            && let Some((old, material)) = state.seen_order.pop_front()
        {
            state.seen.remove(&old);
            // Give the counts back with the digest that took them, so neither map can outlive the
            // FIFO that feeds it.
            if let Some((peer, claim)) = material {
                if let Some(count) = state.materials_per_claim.get_mut(&claim) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        state.materials_per_claim.remove(&claim);
                    }
                }
                if let Some(count) = state.materials_per_peer_claim.get_mut(&(peer, claim)) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        state.materials_per_peer_claim.remove(&(peer, claim));
                    }
                }
            }
        }
        PalwGossipAdmit::Fresh
    }

    /// Admit a material broadcast. `Fresh` means: push [`PalwGossipEvent::Material`] to the inbox
    /// (done here) and the caller should relay the message to its other peers.
    pub fn admit_material(&self, peer: PeerKey, claim: Hash64, bytes: &[u8]) -> PalwGossipAdmit {
        if bytes.len() > PALW_MATERIAL_MAX_BYTES {
            return PalwGossipAdmit::TooBig;
        }
        let digest = self.digest(1, Some(&claim), bytes);
        let verdict = self.admit_digest(digest, Some((peer, claim)), self.is_solicited(claim));
        // Drop-on-full: a node with no consumer is not obliged to remember.
        if verdict == PalwGossipAdmit::Fresh && self.has_consumer() {
            // **A payload the inbox refused must not leave its digest marked seen.** The digest is
            // recorded before the bytes are offered, because the verdict is what the caller relays
            // on — and `try_send` on a full inbox drops the newest, not the oldest. So a single
            // burst (or a slow tick) used to make this node permanently blind to that claim: the
            // pull built for exactly this case re-fetches the same bytes, they hash to the same
            // digest, and `admit_digest` answers `Duplicate` for the next `SEEN_CAP` messages. The
            // seat then holds no material, signs `Unavailable` at the half-window, and an honest
            // producer is defaulted by a quorum.
            if self.inbox_tx.try_send(PalwGossipEvent::Material { claim, bytes: bytes.to_vec() }).is_err() {
                self.forget_digest(digest);
                return PalwGossipAdmit::Duplicate;
            }
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
        // Charged to NO peer and to no claim slot: this node is not a remote flooder of its own
        // claim, and the counters exist to bound what remote peers may occupy (audit3 H7). The
        // digest is still recorded, which is what stops the echo being re-admitted.
        let _ = self.admit_digest(self.digest(1, Some(&claim), bytes), None, true);
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
        assert_eq!(center.admit_material(peer(1), claim, b"first"), PalwGossipAdmit::Fresh, "relay is not gated on having a consumer");
        assert_eq!(center.admit_material(peer(1), claim, b"first"), PalwGossipAdmit::Duplicate, "and dedup still works");
        assert!(!center.has_consumer());

        // Nothing was queued, so the receiver a late-starting service takes is empty rather than
        // holding what arrived while nobody was reading.
        let mut rx = center.take_inbox().expect("the first taker gets it");
        assert!(rx.try_recv().is_err(), "a bystander must not have accumulated payloads");

        // ...and once somebody IS reading, the same call queues.
        assert_eq!(center.admit_material(peer(1), Hash64::from_u64_word(8), b"second"), PalwGossipAdmit::Fresh);
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
            let _ = center.admit_digest(digest, Some((peer(1), claim)), false);
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

    /// Distinct requesting peers, so the per-peer bounds can be exercised.
    fn peer(n: u128) -> PeerKey {
        PeerKey::new(
            kaspa_utils::networking::PeerId::new(uuid::Uuid::from_u128(n)),
            std::net::IpAddr::from([10, 0, 0, (n % 250) as u8 + 1]).into(),
        )
    }

    /// **One peer cannot spend the node's whole serve allowance** (audit3 H5/H10).
    ///
    /// The budget was global and keyed on nothing, so the requester picked the claims and paid
    /// nothing: at the shipped 9.7 MB material, 64 MiB/60 s was 6.9 serves per MINUTE for the whole
    /// node, and seven ~80-byte requests — 560 bytes — silenced every honest seat's pull for the
    /// rest of every window. The seats then sign `Unavailable` and three of five slash the honest
    /// producer's bond. A per-peer share is the bound that matters, because an attacker controls
    /// its own connections and not the node's other peers.
    #[test]
    fn one_peer_cannot_spend_the_whole_serve_budget() {
        let center = PalwGossipCenter::default();
        let flooder = peer(1);
        let honest = peer(2);

        // Drain the flooder's own share, one worst-case reservation at a time.
        let per_peer_reservations = SERVE_BUDGET_BYTES_PER_PEER / PALW_MATERIAL_MAX_BYTES as u64;
        for i in 0..per_peer_reservations {
            assert!(
                center.reserve_serve_budget(flooder, PALW_MATERIAL_MAX_BYTES as u64),
                "reservation {i} is within the peer's share"
            );
        }
        assert!(
            !center.reserve_serve_budget(flooder, PALW_MATERIAL_MAX_BYTES as u64),
            "the flooder is out of allowance once its own share is spent"
        );
        // …and the node still answers everybody else. This is the assertion that fails against a
        // single global counter.
        assert!(
            center.reserve_serve_budget(honest, PALW_MATERIAL_MAX_BYTES as u64),
            "an honest peer's pull is unaffected by what another peer spent"
        );

        // The node-wide figure is still a backstop behind the per-peer share.
        assert!(SERVE_BUDGET_BYTES_PER_WINDOW > SERVE_BUDGET_BYTES_PER_PEER, "the backstop must be larger than one peer's share");
    }

    /// **A refused serve still costs the asker its throttle window** (audit3 H6).
    ///
    /// `served_recently` was written only after a SUCCESSFUL serve, so a request the budget refused
    /// was never recorded — and the identical request could be repeated immediately and forever,
    /// each repeat buying a full synchronous read of up to 16 MiB with zero outbound traffic to
    /// make it visible. Charging the throttle on the attempt is what bounds the read rate.
    #[tokio::test]
    async fn a_serve_that_is_refused_still_charges_the_throttle() {
        let center = PalwGossipCenter::default();
        let claim = h64(11);
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = reads.clone();
        // A resolver that counts how many times the "disk" was touched.
        center.set_material_resolver(std::sync::Arc::new(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(vec![0u8; 1024])
        }));

        assert!(center.resolve_material_for_serve(peer(1), claim).await.is_some(), "the first ask is answered");
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Immediately again — and from a DIFFERENT peer, so this is the claim throttle rather than
        // the per-peer budget doing the work.
        assert!(center.resolve_material_for_serve(peer(2), claim).await.is_none(), "the claim is inside its throttle window");
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1, "and the refusal did NOT touch the disk");

        // **The budget refusal, which is where both halves of the old ordering showed.** Exhaust a
        // peer's allowance and ask it for a claim nothing has touched yet.
        let other = h64(12);
        while center.reserve_serve_budget(peer(3), PALW_MATERIAL_MAX_BYTES as u64) {}
        assert!(center.resolve_material_for_serve(peer(3), other).await.is_none(), "no allowance, no serve");

        // (a) The disk was never touched. The old order was read-then-budget, so an 80-byte request
        //     bought a full synchronous read of up to 16 MiB that produced no bytes at all.
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1, "the allowance is consulted BEFORE the disk");
        // (b) And the refusal was RECORDED. The old code wrote `served_recently` only on success,
        //     so a refused claim was never throttled and the identical request could be repeated
        //     immediately and indefinitely, with zero outbound traffic to make it visible.
        assert!(
            center.served_recently.lock().unwrap().contains_key(&other),
            "a refused serve still costs the asker its throttle window, or the refusal is free and repeatable"
        );
    }

    /// **A stranger must not be able to decide how much memory this node uses** (audit
    /// 2026-09-02).
    ///
    /// `served_recently` is keyed by a claim id taken straight off the wire — no on-chain
    /// existence check, no signature, no bond binding — and the sweep beside it walked the WHOLE
    /// map on every request under a `std::sync::Mutex` every runtime worker can block on. So a
    /// peer naming a fresh random claim per ~70-byte request bought one permanent entry and one
    /// full walk each time: unbounded memory and quadratic CPU, for nothing. The sibling map
    /// `outstanding_pulls`, which only this node writes, was already capped; the peer-written one
    /// was not.
    ///
    /// The requests below are all REFUSED (no resolver is registered, so this node is not a panel
    /// and answers nobody) — which is the point: the map grew on the refusal path, where no
    /// outbound byte was ever produced to make the growth visible.
    #[tokio::test]
    async fn the_serve_throttle_map_cannot_be_grown_without_bound() {
        let center = PalwGossipCenter::default();
        let flooder = peer(1);
        for n in 0..(SERVED_RECENTLY_CAP as u64 + 512) {
            assert!(center.resolve_material_for_serve(flooder, h64(n)).await.is_none(), "no resolver, no serve");
        }
        let held = center.served_recently.lock().unwrap().len();
        assert!(
            held <= SERVED_RECENTLY_CAP,
            "the throttle map held {held} entries against a {SERVED_RECENTLY_CAP} cap — one peer sizing another node's memory"
        );
    }

    /// **A payload the inbox refused must not leave its digest marked seen** (audit 2026-09-02).
    ///
    /// The digest is recorded before the bytes are offered, because the verdict is what the caller
    /// relays on — and `try_send` on a full inbox drops the NEWEST, not the oldest. So one burst
    /// (or one slow tick of the panel service) made this node permanently blind to that claim: the
    /// pull built for exactly this case re-fetches the same bytes, they hash to the same digest,
    /// `admit_digest` answers `Duplicate` for the next `SEEN_CAP` messages, the seat holds no
    /// material, signs `Unavailable` at the half-window, and an honest producer is defaulted by a
    /// quorum.
    ///
    /// The undo has to return the per-claim and per-peer slots with the digest, or the claim's
    /// budget leaks on every drop and the honest answer is crowded out by arithmetic instead.
    #[test]
    fn a_payload_the_full_inbox_dropped_can_be_pulled_again() {
        let center = PalwGossipCenter::default();
        let mut rx = center.take_inbox().expect("first taker");
        // Fill the inbox exactly, one distinct claim per event so nothing here is the per-claim
        // budget doing the work.
        for n in 0..INBOX_CAP as u64 {
            assert_eq!(center.admit_material(peer(1), h64(n), b"payload"), PalwGossipAdmit::Fresh, "event {n} is queued");
        }

        let claim = h64(9_001);
        assert_eq!(
            center.admit_material(peer(2), claim, b"the honest material"),
            PalwGossipAdmit::Duplicate,
            "a payload this node could not keep is not relayed as fresh"
        );
        // …and it left no trace: not the digest, and not the claim's slots.
        {
            let state = center.state.lock().unwrap();
            assert!(!state.materials_per_claim.contains_key(&claim), "the per-claim slot came back with the digest");
            assert!(!state.materials_per_peer_claim.contains_key(&(peer(2), claim)), "and so did the per-peer slot");
        }

        // The service catches up, and the pull re-fetches the identical bytes.
        let mut drained = 0usize;
        while rx.try_recv().is_ok() {
            drained += 1;
        }
        assert_eq!(drained, INBOX_CAP, "the inbox was full, which is the precondition this test needs");

        assert_eq!(
            center.admit_material(peer(2), claim, b"the honest material"),
            PalwGossipAdmit::Fresh,
            "the identical bytes must be admissible again, or the pull that exists to recover from this cannot"
        );
        match rx.try_recv() {
            Ok(PalwGossipEvent::Material { claim: got, .. }) => assert_eq!(got, claim, "and the seat finally holds it"),
            other => panic!("the re-fetched material must reach the consumer, got {other:?}"),
        }
    }

    /// **The solicited exemption is bounded** (audit3 H7).
    ///
    /// It existed so four cheap payloads from a stranger could not make the honest answer
    /// `Duplicate` network-wide, and it was written as a total bypass: for the 120 s TTL every
    /// distinct payload for that claim was `Fresh` and relayed to every peer. That is an amplifier
    /// switched on by the very pull that exists to recover from a flood. It still lets the honest
    /// answer through after the ordinary budget is spent — that is the property M2-1 bought — but
    /// it now stops.
    #[test]
    fn the_solicited_exemption_lets_the_answer_through_and_still_stops() {
        let center = PalwGossipCenter::default();
        let claim = h64(21);
        // Fill the ordinary per-claim budget from two peers, two each.
        for (n, p) in [(1u128, 1usize), (2, 2)] {
            for i in 0..PALW_MATERIALS_PER_PEER_PER_CLAIM {
                assert_eq!(center.admit_material(peer(n), claim, format!("junk-{p}-{i}").as_bytes()), PalwGossipAdmit::Fresh);
            }
        }
        assert_eq!(center.admit_material(peer(3), claim, b"honest"), PalwGossipAdmit::Duplicate, "the ordinary budget is spent");

        // This node asks for it, and now the answer gets in — M2-1's property, preserved.
        center.note_pull_request(claim);
        assert_eq!(center.admit_material(peer(3), claim, b"honest"), PalwGossipAdmit::Fresh, "a solicited answer is not crowded out");

        // But the exemption is not a blank cheque: distinct peers keep feeding it and it stops.
        let mut admitted = 0usize;
        for n in 4..64u128 {
            for i in 0..PALW_MATERIALS_PER_PEER_PER_CLAIM {
                if center.admit_material(peer(n), claim, format!("flood-{n}-{i}").as_bytes()) == PalwGossipAdmit::Fresh {
                    admitted += 1;
                }
            }
        }
        assert!(admitted > 0, "the exemption is real while it lasts");
        let total = center.state.lock().unwrap().materials_per_claim.get(&claim).copied().unwrap_or(0);
        assert_eq!(
            total, PALW_SOLICITED_MATERIALS_PER_CLAIM,
            "the solicited exemption has a ceiling — unbounded, it relays 16 MiB per payload to every peer"
        );
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

        assert_eq!(center.admit_material(peer(1), claim, b"honest"), PalwGossipAdmit::Fresh);
        assert_eq!(center.admit_material(peer(1), claim, b"honest"), PalwGossipAdmit::Duplicate, "relay-once");
        assert_eq!(center.admit_material(peer(1), claim, b"garbage-1"), PalwGossipAdmit::Fresh);
        // **One peer cannot take more than its couple of slots** (audit3 H7). This is the bound
        // that leaves room for somebody else's answer however fast a flooder sends.
        assert_eq!(
            center.admit_material(peer(1), claim, b"garbage-2"),
            PalwGossipAdmit::Duplicate,
            "a third payload for one claim from ONE peer is refused"
        );
        assert_eq!(center.admit_material(peer(2), claim, b"garbage-2"), PalwGossipAdmit::Fresh, "a different peer has its own slots");
        assert_eq!(center.admit_material(peer(2), claim, b"garbage-3"), PalwGossipAdmit::Fresh);
        assert_eq!(
            center.admit_material(peer(3), claim, b"garbage-4"),
            PalwGossipAdmit::Duplicate,
            "the per-claim budget is spent — an attacker cannot make a node relay unboundedly for one claim"
        );
        assert_eq!(center.admit_material(peer(1), h64(8), b"other-claim"), PalwGossipAdmit::Fresh, "another claim has its own budget");

        assert_eq!(center.admit_material(peer(1), h64(9), &vec![0u8; PALW_MATERIAL_MAX_BYTES + 1]), PalwGossipAdmit::TooBig);
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
