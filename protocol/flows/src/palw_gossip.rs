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
    /// **One checkpoint interval's opening, served to THIS node because it asked** (ADR-0077
    /// Decision 8). Unverified bytes: the seat binds them to the claim's roots and replays the
    /// interval before believing anything in them. This layer only checked that the node asked for
    /// exactly this `(claim, interval_index)`, bounded the size before any decode, and
    /// deduplicated.
    IntervalOpening { claim: Hash64, interval_index: u32, bytes: Vec<u8> },
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
    /// An interval opening this node never asked for (ADR-0077 Decision 8). The lane is
    /// asker-specific by design: an opening is bytes exactly one peer wanted, so one that arrives
    /// unasked is a stale answer to an expired pull or a stranger filling a queue. Dropped, and
    /// never relayed — there is nobody to relay it to.
    Unsolicited,
    /// **A `PanelDa` (mode-2) material** (ADR-0077 Decision 16; private-prompts design,
    /// 2026-09-05). The prompt inside it is exactly what the commitment withheld from the chain,
    /// so it is NEVER relayed: a copy this node asked for (a seat's own pull) is handed to the
    /// inbox and stops here; an unasked one is dropped without a digest. The verdict is named
    /// rather than folded into `Duplicate` so the flow's relay condition (`Fresh`) cannot be
    /// widened by a later edit into forwarding it.
    Private,
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
    /// amplification. Keyed by claim, bounded by [`SERVED_RECENTLY_CAP`], evicted oldest-first.
    served_recently: Mutex<ServeThrottle>,
    /// What this node has ASKED for and not yet received, with the time it asked.
    ///
    /// A solicited answer must not be refused by the per-claim relay budget (audit M2-1): four
    /// ~70-byte payloads from a stranger spend that budget before the honest multi-megabyte
    /// material arrives, and then the very pull that exists to recover from it is dropped too.
    /// Bounded by count and by TTL, and only this node's own requests ever write it.
    outstanding_pulls: Mutex<HashMap<Hash64, std::time::Instant>>,
    /// How many bytes this node has emitted answering pulls in the current window.
    serve_budget: Mutex<ServeBudget>,
    /// **The interval lane's whole state** (ADR-0077 Decision 8) — one field rather than six, so
    /// that the lane's bookkeeping can be read, reviewed and changed without touching the
    /// material lane's. See [`PalwOpeningLane`].
    openings: PalwOpeningLane,
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
///
/// **Bounding it with `clear()` was an off-switch, not a bound** (audit 2026-09-02). The first
/// repair swept at the cap and, when the sweep freed nothing — every entry still inside its
/// 60 s window, which is what a flood of fresh ids produces — dropped the whole map. So ~287 KB
/// of ~70-byte requests erased the throttle record of every HONEST claim as collateral, and
/// audit3 H6's "a refused request is free and repeatable" was back for all of them at once. A
/// bounded map whose overflow behaviour is "forget everything" is a rate limiter with a reset
/// button. Eviction is per entry and oldest-first now — see [`ServeThrottle`].
///
/// **Per-entry eviction was not a bound either, and this is the arithmetic that is** (audit
/// 2026-09-02 round 3). Evicting one entry per insert is the same erasure as `clear()` when the
/// inserts are free: a flood of exactly this many fresh ids walks every honest record out of the
/// map one at a time, for the same ~287 KB. The record used to be written BEFORE the budget was
/// reserved, so a peer whose share was already spent went on inserting at no cost at all.
///
/// It is written after the reservation now, which prices one eviction at
/// [`SERVE_ATTEMPT_FLOOR_BYTES`] of the same allowance a real serve spends. Two consequences, and
/// it is the second that bounds it:
///
///   * one peer's share buys ≈513 records (48 MiB, less the 16 MiB reservation that must still fit
///     alongside it), against the 4096 it takes to evict anybody at all;
///   * node-wide, admission stops at (256 MiB − 16 MiB) / 64 KiB = 3841 records per 60 s window —
///     BELOW this cap. No number of sybil peer keys can fill the map inside one window, which is
///     what `the_serve_throttle_cannot_be_flushed_by_sybil_peers_either` asserts.
///
/// **The residual, stated rather than claimed away.** A burst straddling a window boundary gets
/// two allowances (7680 records) inside one [`SERVE_THROTTLE`] = 10 s lifetime, and that CAN evict
/// an honest record. What it cannot do is profit: those admissions are the whole node's serve
/// allowance for two windows, and the read a flushed record would have permitted must be bought
/// out of the same 256 MiB the flush just spent. Eviction and harm are priced in one currency at
/// one unit price, so a flush is self-defeating rather than an off-switch — which is the whole
/// difference from `clear()`, where one ~70-byte message bought every eviction there was.
const SERVED_RECENTLY_CAP: usize = 4096;
/// **What one serve ATTEMPT costs its asker, whatever the attempt produces.**
///
/// The reservation above is the worst case and the difference comes back once the real size is
/// known — so the per-peer share bounded BYTES and not READS: at a 1 KiB material a peer's 48 MiB
/// bought ~49,000 synchronous disk reads a minute, and a read that produced nothing (no resolver,
/// nothing on disk, bytes over the cap) was refunded in full and therefore free. What this node
/// actually spends on a request is a blocking-pool thread and a `std::fs::read`, and that is a
/// per-REQUEST cost, so the ceiling has to be one too.
///
/// 48 MiB / 64 KiB = 768 attempts per peer per minute is the ceiling this sets, and the real figure
/// is lower still — the 16 MiB worst-case reservation must also fit in what is left of the share,
/// so a peer is refused at ~513. Either number is far above five seats pulling one claim on a
/// 25-DAA throttle (~50 minutes at the frozen cadence) and far below what a request loop conjures.
/// The property that matters is that it is keyed by `PeerKey` rather than by an attacker-chosen
/// claim id, so no message can flush it. Where a real material is served the floor is invisible:
/// every registered class's material is orders of magnitude larger than it.
const SERVE_ATTEMPT_FLOOR_BYTES: u64 = 64 << 10;
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
/// How many bytes of unsolicited material one peer may have relayed per [`PALW_MATERIAL_BUDGET_WINDOW`]
/// (mainnet audit, 2026-09-05): four maximal materials, which is the per-claim budget itself — a
/// peer that is not answering a pull has no honest reason to exceed it across ANY set of claims.
pub const PALW_MATERIAL_BYTES_PER_PEER_PER_WINDOW: u64 = 4 * PALW_MATERIAL_MAX_BYTES as u64;
/// The window [`PALW_MATERIAL_BYTES_PER_PEER_PER_WINDOW`] is measured over.
pub const PALW_MATERIAL_BUDGET_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

/// **The serve throttle's memory, bounded per entry.**
///
/// A `HashMap` for the lookup and a FIFO of the keys for the eviction: at the cap the OLDEST
/// recorded claim is dropped, one of them, in `O(1)`. That is the difference between "a flood
/// costs the flooder's own oldest record" and "a flood costs every honest claim's record at
/// once", which is what `clear()` bought. There is no periodic sweep either — a stale entry is
/// simply outside its window when it is read, and 4096 of them is the whole memory bill — so no
/// request walks the map under the lock any more.
#[derive(Default)]
struct ServeThrottle {
    at: HashMap<Hash64, std::time::Instant>,
    order: VecDeque<Hash64>,
}

impl ServeThrottle {
    fn recorded_at(&self, claim: &Hash64) -> Option<std::time::Instant> {
        self.at.get(claim).copied()
    }

    /// Charge `claim` its window. A claim already recorded keeps its place in the FIFO and only
    /// refreshes its time: moving it would let a peer pin an entry by re-asking, which is the
    /// behaviour the throttle exists to charge for.
    fn record(&mut self, claim: Hash64, now: std::time::Instant) {
        if self.at.insert(claim, now).is_none() {
            self.order.push_back(claim);
            while self.order.len() > SERVED_RECENTLY_CAP {
                let Some(oldest) = self.order.pop_front() else { break };
                self.at.remove(&oldest);
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.at.len()
    }
}

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
    /// **Bytes of UNSOLICITED material one peer has been allowed to relay in the current window**
    /// (mainnet audit, 2026-09-05). The per-claim and per-peer-per-claim counts above are keyed by
    /// a claim id the peer chooses, so a peer naming a fresh id per payload sat under every one of
    /// them and had each payload — up to 16 MiB — relayed to every other peer. This is the bound
    /// the id cannot move: a peer's relay budget in bytes, per window, whatever it calls the claim.
    material_bytes_per_peer: HashMap<PeerKey, (std::time::Instant, u64)>,
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
                material_bytes_per_peer: HashMap::new(),
            }),
            inbox_tx,
            inbox_rx: Mutex::new(Some(inbox_rx)),
            inbox_taken: std::sync::atomic::AtomicBool::new(false),
            hasher: RandomState::new(),
            material_resolver: Mutex::new(None),
            served_recently: Mutex::new(ServeThrottle::default()),
            outstanding_pulls: Mutex::new(HashMap::new()),
            serve_budget: Mutex::new(ServeBudget {
                window_started: std::time::Instant::now(),
                bytes_served: 0,
                per_peer: HashMap::new(),
            }),
            openings: PalwOpeningLane::default(),
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
        // **An unauthenticated pull is a stranger's** (ADR-0077 Decision 16): a private material
        // is not served on it, whoever holds it. The bytes were read and the window charged, so
        // this refusal is priced exactly like a served answer and cannot be probed for free.
        self.resolve_material_bytes(peer, claim).await.filter(|bytes| !material_is_private(bytes))
    }

    /// The whole-capture read every serve lane shares — throttled, budgeted and bounded, with no
    /// opinion about who is asking. The two public entries above and below decide that.
    async fn resolve_material_bytes(&self, peer: PeerKey, claim: Hash64) -> Option<Vec<u8>> {
        {
            let served = self.served_recently.lock().unwrap();
            if let Some(at) = served.recorded_at(&claim)
                && std::time::Instant::now().duration_since(at) < SERVE_THROTTLE
            {
                return None;
            }
        }
        // Reserve the worst case; the difference comes back once the real size is known.
        let reservation = PALW_MATERIAL_MAX_BYTES as u64;
        if !self.reserve_serve_budget(peer, reservation) {
            return None;
        }
        // **The window is written by a request this node's own budget ADMITTED, and that is what
        // makes the map unfloodable** (audit 2026-09-02 round 3).
        //
        // Recording before the reservation made the eviction free: a peer whose 48 MiB share was
        // spent kept inserting a fresh id per ~70-byte request, at no further cost, and
        // `SERVED_RECENTLY_CAP` of those evict every honest claim's record one at a time — the
        // same outcome `clear()` had, for ~287 KB instead of one message. Past the reservation
        // every attempt also costs [`SERVE_ATTEMPT_FLOOR_BYTES`], so writing the map here prices
        // the eviction in the one currency nothing on the wire can reset: 48 MiB / 64 KiB ≈ 513
        // records per peer per window, and 256 MiB / 64 KiB = 4096 node-wide per 60 s. Evicting
        // somebody else's record takes `SERVED_RECENTLY_CAP` = 4096 fresh ones, and the record
        // only has to survive `SERVE_THROTTLE` = 10 s — inside which the node-wide ceiling admits
        // ~682. The flush is arithmetically out of reach by a factor of six, from every peer at
        // once, rather than costing one flooder a few hundred kilobytes.
        //
        // Audit3 H6's property is untouched: everything PAST the reservation — no resolver,
        // nothing on disk, bytes over the cap — is still charged the window here, before the read,
        // so a refusal that cost this node a disk seek is never free and repeatable. The only
        // unrecorded path is the peer's own exhausted share, which costs this node two mutex
        // acquisitions and no I/O, and which the peer cannot escape until the window turns.
        //
        // The check and the write are no longer one critical section, so two requests for the same
        // fresh claim arriving together can both proceed. Each pays the floor, so the duplicate is
        // bounded by the budget — which the doc above already names as the bound that survives the
        // claim throttle being defeated at all.
        {
            let mut served = self.served_recently.lock().unwrap();
            served.record(claim, std::time::Instant::now());
        }
        // **Past the reservation every exit costs the asker [`SERVE_ATTEMPT_FLOOR_BYTES`].** The
        // refund used to return the reservation minus the bytes actually sent, and the whole
        // reservation on every path that sent none — so a small material, or a claim this node
        // does not hold, made the read free and the per-peer share bounded nothing that costs
        // this node anything. The floor is what makes the share a bound on READS, and it is the
        // one that still holds when the claim throttle above has been flushed.
        let settle = |served: u64| self.refund_serve_budget(peer, reservation.saturating_sub(served.max(SERVE_ATTEMPT_FLOOR_BYTES)));
        // **The disk read happens with NO lock held and NOT on an async worker.** The closure reads
        // up to 16 MiB synchronously; holding the global resolver mutex across it (as this did
        // until audit M2-2) serialises every serve on the slowest one, and running it inline on the
        // shared tokio runtime (as it did until audit3 H6) lets an 80-byte request pin a reactor
        // thread.
        let resolver = { self.material_resolver.lock().unwrap().clone() };
        let Some(resolver) = resolver else {
            settle(0);
            return None;
        };
        let read = tokio::task::spawn_blocking(move || resolver(claim)).await;
        let Some(bytes) = read.ok().flatten() else {
            settle(0);
            return None;
        };
        if bytes.len() > PALW_MATERIAL_MAX_BYTES {
            settle(0); // never serve what the transport would refuse
            return None;
        }
        settle(bytes.len() as u64);
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
    ///
    /// **Kept beside [`Self::inbox_permit`], which replaced it on the material and receipt lanes.**
    /// Reserving the slot first is the version of this with nothing to undo, and it is strictly
    /// better; the INTERVAL lane (added after that repair) still admits its digest before it takes
    /// its per-interval slot, because that order is what stops a repeat of one payload from
    /// spending four slots, and it needs an undo for the two paths that then refuse. So this scan
    /// survives on that lane alone, and the audit's objection to it — a full `seen_order` walk and
    /// memmove inside the gossip mutex — survives with it. Reworking the interval lane onto a
    /// reservation is the follow-up; it is not a merge.
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

    /// **An inbox slot, taken BEFORE anything is recorded.**
    ///
    /// `Ok(None)` on a node with no consumer: it relays and deduplicates for the network without
    /// keeping a copy, and owes nobody one. `Err(())` when the inbox is full, and the caller must
    /// then record NOTHING — see [`Self::admit_material`].
    ///
    /// This replaced an undo. The first repair recorded the digest, offered the bytes, and on a
    /// failed `try_send` walked `seen_order` from the FRONT looking for the entry it had pushed to
    /// the BACK one instruction earlier — a full 4096-element scan and a 96 KB memmove per inbound
    /// message, inside the one `Mutex` that serialises every peer's gossip, on exactly the path an
    /// attacker takes by keeping the inbox full. That is the shape the sibling fix in the same
    /// commit condemns in the serve throttle. Reserving first means there is no undo to make
    /// O(1): a payload this node cannot keep never becomes a digest it remembers.
    fn inbox_permit(&self) -> Result<Option<mpsc::Permit<'_, PalwGossipEvent>>, ()> {
        if !self.has_consumer() {
            return Ok(None);
        }
        self.inbox_tx.try_reserve().map(Some).map_err(|_| ())
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
    /// **Charge `len` bytes of unsolicited material to `peer`'s window budget; `false` means the
    /// budget is spent** (mainnet audit, 2026-09-05). Windows are per peer and start at the peer's
    /// first charge; a window older than [`PALW_MATERIAL_BUDGET_WINDOW`] is dropped on the next
    /// charge from anyone, so the map holds at most one entry per peer that spoke this minute.
    fn charge_material_budget(&self, peer: PeerKey, len: u64) -> bool {
        let mut state = self.state.lock().unwrap();
        let now = std::time::Instant::now();
        state.material_bytes_per_peer.retain(|_, (since, _)| now.duration_since(*since) < PALW_MATERIAL_BUDGET_WINDOW);
        let (_, spent) = state.material_bytes_per_peer.entry(peer).or_insert((now, 0));
        if spent.saturating_add(len) > PALW_MATERIAL_BYTES_PER_PEER_PER_WINDOW {
            return false;
        }
        *spent += len;
        true
    }

    pub fn admit_material(&self, peer: PeerKey, claim: Hash64, bytes: &[u8]) -> PalwGossipAdmit {
        if bytes.len() > PALW_MATERIAL_MAX_BYTES {
            return PalwGossipAdmit::TooBig;
        }
        // **A payload the inbox cannot hold must not leave a digest behind.** The digest is what
        // makes the next sighting a `Duplicate`, and `try_send` on a full inbox drops the newest,
        // not the oldest — so one burst (or one slow tick of the panel service) made this node
        // permanently blind to that claim: the pull built for exactly this case re-fetches the
        // same bytes, they hash to the same digest, and `admit_digest` answers `Duplicate` for the
        // next `SEEN_CAP` messages. The seat then holds no material, signs `Unavailable` at the
        // half-window, and an honest producer is defaulted by a quorum.
        //
        // Taking the slot FIRST is the version of that with nothing to undo, and no counters
        // charged to a claim whose payload was never kept.
        let Ok(permit) = self.inbox_permit() else { return PalwGossipAdmit::Duplicate };
        // The budget the claim id cannot move: an answer this node asked for is exempt (it is
        // still digest-deduplicated, size-capped and bounded per claim); everything else spends
        // the peer's window budget before it is even hashed, and a spent budget is `Duplicate` —
        // dropped, not relayed.
        let solicited = self.is_solicited(claim);
        if !solicited && !self.charge_material_budget(peer, bytes.len() as u64) {
            return PalwGossipAdmit::Duplicate;
        }
        // **A private material stops at the node that asked for it** (ADR-0077 Decision 16). The
        // budget above is charged either way — an unasked mode-2 payload is still a peer spending
        // this node's window — but no digest is written for it and nothing is relayed: the next
        // honest sighting must not read as a `Duplicate` of a copy this node refused to keep.
        if material_is_private(bytes) {
            if !solicited {
                return PalwGossipAdmit::Private;
            }
            let digest = self.digest(1, Some(&claim), bytes);
            if self.admit_digest(digest, Some((peer, claim)), true) == PalwGossipAdmit::Fresh
                && let Some(permit) = permit
            {
                permit.send(PalwGossipEvent::Material { claim, bytes: bytes.to_vec() });
            }
            return PalwGossipAdmit::Private;
        }
        let digest = self.digest(1, Some(&claim), bytes);
        let verdict = self.admit_digest(digest, Some((peer, claim)), solicited);
        // No permit means no consumer, which is most nodes: they relay and deduplicate for the
        // network without keeping a copy.
        if verdict == PalwGossipAdmit::Fresh
            && let Some(permit) = permit
        {
            permit.send(PalwGossipEvent::Material { claim, bytes: bytes.to_vec() });
        }
        verdict
    }

    /// Admit a receipt broadcast. Same contract as [`Self::admit_material`].
    pub fn admit_receipt(&self, bytes: &[u8]) -> PalwGossipAdmit {
        if bytes.len() > PALW_RECEIPT_MAX_BYTES {
            return PalwGossipAdmit::TooBig;
        }
        // Same order and the same reason: a receipt the inbox refused used to stay marked seen, so
        // the re-broadcast that would have delivered it was a `Duplicate` and the quorum was one
        // signature short for no reason a node could observe.
        //
        // The trade is the one `admit_material` already makes: a full inbox now stops the RELAY
        // too, not just the copy. It costs a relay hop on panel nodes whose service is momentarily
        // behind — every node without the role has no inbox to fill and relays unaffected — and it
        // buys back the property that a payload this node could not keep can still arrive.
        let Ok(permit) = self.inbox_permit() else { return PalwGossipAdmit::Duplicate };
        let verdict = self.admit_digest(self.digest(2, None, bytes), None, false);
        if verdict == PalwGossipAdmit::Fresh
            && let Some(permit) = permit
        {
            permit.send(PalwGossipEvent::Receipt { bytes: bytes.to_vec() });
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
        // **The slot first, and on this path the ordering decides a court case.** Marked-then-sent
        // left a producer whose panel service was a moment behind PERMANENTLY blind to its own
        // capture, with no way back at all: the echo a peer relays is the identical bytes, so it
        // hashes to the identical digest and `admit_digest` answers `Duplicate`; a
        // `PalwMaterialRequest` pull returns the same bytes with the same result; and re-running
        // the execution is deterministic, so re-marking is a no-op. The doc above names what that
        // costs — a dispute is opened against the executor and the executor has nothing to answer
        // with. Not recording the digest when the payload could not be kept is what leaves the
        // echo admissible.
        let Ok(permit) = self.inbox_permit() else { return };
        // Charged to NO peer and to no claim slot: this node is not a remote flooder of its own
        // claim, and the counters exist to bound what remote peers may occupy (audit3 H7). The
        // digest is still recorded, which is what stops the echo being re-admitted.
        let _ = self.admit_digest(self.digest(1, Some(&claim), bytes), None, true);
        if let Some(permit) = permit {
            permit.send(PalwGossipEvent::Material { claim, bytes: bytes.to_vec() });
        }
    }

    pub fn mark_own_receipt(&self, bytes: &[u8]) {
        let _ = self.admit_digest(self.digest(2, None, bytes), None, true);
    }
}

// ---------------------------------------------------------------------------------------------
// The interval lane (ADR-0077 Decision 8, SA-2; ADR-0079 Decision 4's DA-opening-server row, SA-3)
// ---------------------------------------------------------------------------------------------

/// **Hard cap on one checkpoint-interval opening** (ADR-0077 Decision 8, invariant W10).
///
/// An opening is `O(interval × row + log₂ leaves)` by construction — the checkpoint chunk at the
/// interval's start, the committed rows of one interval, the ids it consumed and produced, and the
/// Merkle paths that bind them — so it is bounded by the class's checkpoint geometry and never by
/// `decode_tokens_executed`. A quarter of the whole-capture cap: the point of the lane is that a
/// seat fetches a bounded slice, and an "opening" the size of the capture would be the old
/// transport under a new name. The same rule as `PALW_MATERIAL_MAX_BYTES` applies in the other
/// direction — a transport cap must never overrule an admission the consensus accepted — so a
/// family whose widest registered row opens larger than this raises it HERE, before the class is
/// registered. The seat applies its own, tighter, per-class ceiling on top
/// (`palw_fp_interval_opening_ceiling_v1`); this is the transport's backstop.
pub const PALW_INTERVAL_OPENING_MAX_BYTES: usize = 4 << 20;
/// Distinct opening payloads admitted per solicited `(claim, interval)`. The honest executor needs
/// one; a forger who knows a live claim id and guesses a drawn index can make this seat attempt at
/// most this many replays before the slot is full.
pub const PALW_OPENINGS_PER_INTERVAL: usize = 4;
/// No single peer may take more than a couple of an interval's slots — the audit3 H7 rule,
/// carried: with four slots and a per-peer ceiling of two there is always room for the executor's
/// answer however fast a flooder sends.
pub const PALW_OPENINGS_PER_PEER_PER_INTERVAL: usize = 2;
/// How long one `(peer, claim, interval)` stays un-servable after a serve is ATTEMPTED for it. A
/// seat re-asks on a 25-DAA cadence (~50 minutes at the frozen 120 s cadence), so anything inside
/// this window is a loop and not a seat.
const INTERVAL_SERVE_THROTTLE: std::time::Duration = std::time::Duration::from_secs(10);
/// The per-BOND request rate (ADR-0077 SA-2's "rate-limited per bond"). A seat draws
/// `PALW_FP_SEAT_INTERVAL_SAMPLES_V1` = 4 intervals per claim it is seated on and re-asks on a
/// 25-DAA throttle, and a challenger opens one claim at a time, so this is far above any honest
/// need over a minute and far below what a request loop conjures. Charged only to a requester the
/// authorizer already mapped to a bond, so an unbonded flooder never reaches it.
const OPENING_REQUESTS_PER_BOND_PER_WINDOW: u32 = 128;
/// The rate map's own bound. Entries are bonds — each one collateral somebody posted — and the map
/// is cleared with the window, so this only bites when more distinct bonds ask in one minute than
/// any live panel has seats. Refusing beyond it is the fail-closed side of a choice that has no
/// safe fail-open side: a map keyed by anything unbounded is memory a stranger sizes.
const OPENING_RATE_BONDS_CAP: usize = 4_096;
/// The per-PEER request rate that sits IN FRONT of the signature verification. Higher than the
/// per-bond rate because one connection may legitimately carry several bonds' asks (an operator
/// running two seats behind one node), and it is not the serving bound — it is the bound on how
/// much lattice verification and chain reading one connection can buy.
const OPENING_REQUESTS_PER_PEER_PER_WINDOW: u32 = 512;
/// How far a request's `requested_daa` may sit from the server's own view before it is stale. A
/// signature that never expired would be a permanent serving right the requester could sell or
/// leak; this makes it a window. Checked by the authorizer, which is the only party here with a
/// DAA view.
pub const OPENING_REQUEST_FRESHNESS_DAA: u64 = 200;

/// **An authenticated request for served data** (ADR-0077 SA-2, ADR-0079 SA-3): the interval lane
/// and, since SA-2's last sentence, the whole-capture pull too.
///
/// Borrowed rather than owned because it is built from a decoded message and consumed inside one
/// call — nothing here is stored, and the parts that ARE stored (the bond id, the claim) are
/// copied out by the authorizer.
#[derive(Clone, Copy, Debug)]
pub struct PalwOpeningRequestV1<'a> {
    pub claim: Hash64,
    /// The interval asked for, or `None` for a whole-capture pull. It is part of the signed
    /// message, so one signature cannot be replayed as a request for every interval of a claim.
    pub interval_index: Option<u32>,
    /// The requester's DAA score when it signed.
    pub requested_daa: u64,
    /// The requester's ML-DSA-87 public key. The bond is DERIVED from it on chain; a carried bond
    /// key would be the requester's own claim about itself.
    pub requester_pubkey: &'a [u8],
    pub signature: &'a [u8],
}

/// Why a serve was refused — **by name**, because "the seat heard nothing" and "the seat was
/// refused" are different facts and only one of them is the executor's fault. A refusal is
/// returned to the caller and logged by the server; it is never sent to the requester, which would
/// make the refusal itself an amplifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwServeRefusalV1 {
    /// No key or no signature at all: a pre-SA-2 requester, or an amplifier probe.
    Unsigned,
    /// A key or signature that is not the shape ML-DSA-87 produces. Checked before ANY other work
    /// — this is ADR-0079 Decision 4's `check_opening_request_shape`.
    Malformed,
    /// The signature does not verify under the carried key, or does not cover this request.
    BadSignature,
    /// The key verifies but the chain maps it to no bond that may ask: not a seat of this claim's
    /// panel and not an Active bond.
    NotBonded,
    /// The signature is outside [`OPENING_REQUEST_FRESHNESS_DAA`] of the server's own view.
    Stale,
    /// This bond has spent its per-window request allowance.
    RateLimited,
    /// This peer asked for the same thing inside the throttle window.
    Throttled,
    /// The window's byte allowance for this peer, or for the node, is spent.
    NoAllowance,
    /// This node holds nothing it can open for that claim — the honest silence of a node that is
    /// not the executor and never heard the capture.
    NotHeld,
    /// What the opener produced is larger than the transport would carry, so emitting it would
    /// only spend this node's egress on bytes the far end drops.
    Oversized,
    /// The requester is bonded and honest about it, and is still not one of the bonds this
    /// claim's private material may be served to — not its executor, not a seat of its panel,
    /// not the challenger of a session open on it (ADR-0077 Decision 16's transport half). Also
    /// the answer to an UNSIGNED pull for such a material: a stranger cannot be a reader.
    NotAReader,
}

impl PalwServeRefusalV1 {
    /// A stable name for logs and tests. `Display` is deliberately not implemented: this string is
    /// a wire-visible-ish operational fact, and a `Display` impl invites it into an error chain
    /// where it would be reformatted.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unsigned => "unsigned",
            Self::Malformed => "malformed",
            Self::BadSignature => "bad-signature",
            Self::NotBonded => "not-bonded",
            Self::Stale => "stale",
            Self::RateLimited => "rate-limited",
            Self::Throttled => "throttled",
            Self::NoAllowance => "no-allowance",
            Self::NotHeld => "not-held",
            Self::Oversized => "oversized",
            Self::NotAReader => "not-a-reader",
        }
    }
}

/// ML-DSA-87's public key and signature sizes, spelled here so the shape check needs no crypto
/// dependency in the transport crate. Decision 4's row is explicit that the DA opening server
/// "parses opening requests from any seat" and "holds the capture, read-only": the process that
/// parses a stranger's bytes holds no key, so the verification itself belongs to the node's panel
/// service, which does hold one, and reaches this layer as the registered authorizer.
const MLDSA87_PUBKEY_BYTES: usize = 2_592;
const MLDSA87_SIGNATURE_BYTES: usize = 4_627;

/// **The cheapest gate, run before anything is read** (ADR-0079 Decision 4).
///
/// Length equality, not a bound: an ML-DSA-87 key is exactly one size and a signature is exactly
/// one size, so anything else is not a request that could ever verify, and rejecting it here costs
/// two comparisons instead of a chain read and a lattice verification.
pub fn check_opening_request_shape(request: &PalwOpeningRequestV1<'_>) -> Result<(), PalwServeRefusalV1> {
    if request.requester_pubkey.is_empty() || request.signature.is_empty() {
        return Err(PalwServeRefusalV1::Unsigned);
    }
    if request.requester_pubkey.len() != MLDSA87_PUBKEY_BYTES || request.signature.len() != MLDSA87_SIGNATURE_BYTES {
        return Err(PalwServeRefusalV1::Malformed);
    }
    Ok(())
}

/// What the node's panel service answers when asked "may this key be served, and as which bond?".
///
/// It returns the BOND, because that is the rate-limit key SA-2 names, and because a bond is the
/// thing collateral was posted for — a pubkey is free to generate. `Err` names the refusal.
/// **Is this payload a `PanelDa` (mode-2) material?** — read off the job in its prefix and
/// nothing else (`palw_fp_privacy_mode_peek_v1`). Every transport gate in this module asks this
/// before it announces, relays or serves bytes; what the bytes are worth is the seat's question.
pub fn material_is_private(bytes: &[u8]) -> bool {
    kaspa_consensus_core::palw_freeprompt_v3::palw_fp_privacy_mode_peek_v1(bytes)
        == Some(kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA)
}

pub type PalwOpeningAuthorizer = std::sync::Arc<dyn Fn(&PalwOpeningRequestV1<'_>) -> Result<Hash64, PalwServeRefusalV1> + Send + Sync>;

/// One solicited interval's admitted payloads — the material lane's per-claim and
/// per-peer-per-claim counts, keyed per interval.
#[derive(Default)]
struct OpeningSlots {
    admitted: usize,
    per_peer: HashMap<PeerKey, usize>,
}

struct OpeningRateWindow {
    started: std::time::Instant,
    per_bond: HashMap<Hash64, u32>,
}

/// The same window, keyed on the connection — see [`PalwGossipCenter::authorize_serve`] for why
/// there are two.
struct OpeningPeerRateWindow {
    started: std::time::Instant,
    per_peer: HashMap<PeerKey, u32>,
}

/// **Everything the interval lane owns.** Kept in one struct so the lane can be reviewed as a
/// unit and so the material lane's bookkeeping is untouched by it.
struct PalwOpeningLane {
    /// **Who may be served** (SA-2) — registered by the node's panel service, the only party here
    /// that holds a key and a chain view. `None` on a node that serves nothing, which then refuses
    /// every request, signed or not: a node with no authorizer has no capture to open either.
    authorizer: Mutex<Option<PalwOpeningAuthorizer>>,
    /// **Where an opening comes from** — also the panel service, the party holding the retention
    /// directory AND the family backends that can open a retained capture (`open_fp_interval`).
    /// A closure rather than a file read because an opening is COMPUTED from the capture, not
    /// copied out of it. Runs on a blocking thread with no lock held.
    resolver: Mutex<Option<std::sync::Arc<dyn Fn(Hash64, u32) -> Option<Vec<u8>> + Send + Sync>>>,
    /// Serve throttle, keyed by the ASKER as well as the interval: two seats of one panel
    /// legitimately draw the same interval seconds apart, and a throttle keyed on the interval
    /// alone would answer the first and silence the second. What must not repeat is one peer
    /// asking for one interval in a loop, and that is what this keys on.
    served_recently: Mutex<HashMap<(PeerKey, Hash64, u32), std::time::Instant>>,
    /// The per-bond request rate (SA-2), cleared with its window.
    rate: Mutex<OpeningRateWindow>,
    /// The per-peer rate that bounds the verification work itself.
    peer_rate: Mutex<OpeningPeerRateWindow>,
    /// The `(claim, interval)` pairs this node has ASKED for and not yet heard, with the time it
    /// asked — the only key an opening is admitted under. Bounded by TTL and by
    /// [`OUTSTANDING_PULL_CAP`], and only this node's own requests ever write it.
    outstanding: Mutex<HashMap<(Hash64, u32), std::time::Instant>>,
    /// How many opening payloads each solicited interval has admitted, and from whom. Entries
    /// exist only for solicited pairs and are swept with them, so the map is bounded by
    /// [`OUTSTANDING_PULL_CAP`] by construction.
    slots: Mutex<HashMap<(Hash64, u32), OpeningSlots>>,
}

impl Default for PalwOpeningLane {
    fn default() -> Self {
        Self {
            authorizer: Mutex::new(None),
            resolver: Mutex::new(None),
            served_recently: Mutex::new(HashMap::new()),
            rate: Mutex::new(OpeningRateWindow { started: std::time::Instant::now(), per_bond: HashMap::new() }),
            peer_rate: Mutex::new(OpeningPeerRateWindow { started: std::time::Instant::now(), per_peer: HashMap::new() }),
            outstanding: Mutex::new(HashMap::new()),
            slots: Mutex::new(HashMap::new()),
        }
    }
}

impl PalwGossipCenter {
    /// Register who may be served (SA-2/SA-3) — panel service only. Covers BOTH lanes: once an
    /// authorizer exists, the whole-capture pull is authenticated too, which is SA-2's last
    /// sentence.
    pub fn set_opening_authorizer(&self, authorizer: PalwOpeningAuthorizer) {
        *self.openings.authorizer.lock().unwrap() = Some(authorizer);
    }

    /// Register the opener a serve consults — panel service only; see the field's doc.
    pub fn set_interval_opening_resolver(&self, resolver: std::sync::Arc<dyn Fn(Hash64, u32) -> Option<Vec<u8>> + Send + Sync>) {
        *self.openings.resolver.lock().unwrap() = Some(resolver);
    }

    /// Is this node serving openings at all? (`false` before the panel registers, and on every
    /// node with no PALW role.)
    pub fn serves_openings(&self) -> bool {
        self.openings.resolver.lock().unwrap().is_some()
    }

    /// **The authentication both lanes share** (ADR-0077 SA-2, ADR-0079 SA-3): shape, then a
    /// per-PEER rate, then signature and bond, then the per-BOND rate — cheapest first, and every
    /// one of them before a byte of capture is read.
    ///
    /// **Two rates, because the one SA-2 names cannot be the first gate.** A per-bond rate needs
    /// the bond, and getting the bond means verifying a lattice signature and reading chain state
    /// — so a rate that only exists after them does not bound them. The per-peer rate in front
    /// costs a map lookup, is keyed on something an attacker cannot mint (its own connection), and
    /// is what actually bounds the verification work; the per-bond rate behind it is what bounds
    /// the SERVING work, which is SA-2's subject.
    ///
    /// **The verification runs off the async runtime.** The authorizer verifies ML-DSA-87 and
    /// reads chain state, and this node's public entrance and its seat have already been the same
    /// process once (`t11-5d-public-node-hang-analysis`): a request that pins a reactor thread is
    /// a request that stops block relay.
    ///
    /// A node with no authorizer refuses everything rather than serving everyone: a fail-open
    /// default here would be the amplifier SA-2 exists to close.
    pub async fn authorize_serve(&self, peer: PeerKey, request: &PalwOpeningRequestV1<'_>) -> Result<Hash64, PalwServeRefusalV1> {
        check_opening_request_shape(request)?;
        self.charge_opening_peer_rate(peer)?;
        let authorizer = { self.openings.authorizer.lock().unwrap().clone() };
        let Some(authorizer) = authorizer else { return Err(PalwServeRefusalV1::NotBonded) };
        let (claim, interval_index, requested_daa) = (request.claim, request.interval_index, request.requested_daa);
        let (pubkey, signature) = (request.requester_pubkey.to_vec(), request.signature.to_vec());
        let verified = tokio::task::spawn_blocking(move || {
            authorizer(&PalwOpeningRequestV1 {
                claim,
                interval_index,
                requested_daa,
                requester_pubkey: &pubkey,
                signature: &signature,
            })
        })
        .await;
        // A panicked or cancelled authorizer is a refusal, not a serve: this node does not know
        // who asked.
        let bond = verified.map_err(|_| PalwServeRefusalV1::NotBonded)??;
        self.charge_opening_rate(bond)?;
        Ok(bond)
    }

    /// The cheap rate in front of the crypto, keyed on the connection. Bounded by the peer count,
    /// which the connection manager owns, and cleared with the window.
    fn charge_opening_peer_rate(&self, peer: PeerKey) -> Result<(), PalwServeRefusalV1> {
        let mut rate = self.openings.peer_rate.lock().unwrap();
        let now = std::time::Instant::now();
        if now.duration_since(rate.started) >= SERVE_BUDGET_WINDOW {
            rate.started = now;
            rate.per_peer.clear();
        }
        let spent = rate.per_peer.entry(peer).or_insert(0);
        if *spent >= OPENING_REQUESTS_PER_PEER_PER_WINDOW {
            return Err(PalwServeRefusalV1::RateLimited);
        }
        *spent += 1;
        Ok(())
    }

    /// Spend one of this bond's requests in the current window.
    fn charge_opening_rate(&self, bond: Hash64) -> Result<(), PalwServeRefusalV1> {
        let mut rate = self.openings.rate.lock().unwrap();
        let now = std::time::Instant::now();
        if now.duration_since(rate.started) >= SERVE_BUDGET_WINDOW {
            rate.started = now;
            rate.per_bond.clear();
        }
        let known = rate.per_bond.contains_key(&bond);
        if !known && rate.per_bond.len() >= OPENING_RATE_BONDS_CAP {
            return Err(PalwServeRefusalV1::RateLimited);
        }
        let spent = rate.per_bond.entry(bond).or_insert(0);
        if *spent >= OPENING_REQUESTS_PER_BOND_PER_WINDOW {
            return Err(PalwServeRefusalV1::RateLimited);
        }
        *spent += 1;
        Ok(())
    }

    /// **The authenticated whole-capture serve** (ADR-0077 SA-2's last sentence). Authorize, then
    /// the unchanged [`Self::resolve_material_for_serve`] — the throttle, the per-peer byte
    /// allowance and the blocking read are exactly what they were, with a bond in front of them.
    ///
    /// **A node that registered no authorizer answers as it did before**, deliberately. The panel
    /// service registers the authorizer and the material resolver in the same place, so a node
    /// with no authorizer has nothing to serve and this returns `NotHeld` on its own; making the
    /// authorizer a hard precondition would instead mean that any future refactor which registers
    /// one and not the other silences every honest seat's pull — and a seat with no material signs
    /// `Unavailable`, and three of those slash an honest producer. That failure has happened on
    /// this lane twice (the 8 MiB cap, the per-claim budget), so the authentication is added in
    /// front of the serve rather than made a condition of it existing.
    pub async fn resolve_material_for_serve_signed(
        &self,
        peer: PeerKey,
        request: &PalwOpeningRequestV1<'_>,
    ) -> Result<Vec<u8>, PalwServeRefusalV1> {
        // Bound first, deliberately: a temporary `MutexGuard` in an `if` condition lives to the end
        // of the whole `if` statement, so writing this inline would hold a std mutex across an
        // `.await` — a non-`Send` future at best and a reactor-wide stall at worst.
        let authenticated = { self.openings.authorizer.lock().unwrap().is_some() };
        if authenticated {
            self.authorize_serve(peer, request).await?;
        }
        let bytes = self.resolve_material_bytes(peer, request.claim).await.ok_or(PalwServeRefusalV1::NotHeld)?;
        // **A private material is served only past an authorizer that has said this bond is one
        // of the claim's readers** (ADR-0077 Decision 16). The authorizer is the node's own
        // (`opening_authorizer` in the panel) and it answers `NotAReader` itself for a bond that
        // is not; a node with NO authorizer installed cannot have asked, so it serves nothing
        // private — the same fail-closed reading the unsigned lane has.
        if !authenticated && material_is_private(&bytes) {
            return Err(PalwServeRefusalV1::NotAReader);
        }
        Ok(bytes)
    }

    /// The opening of `interval_index` of `claim`'s retained capture, for a requester this node
    /// has authorized.
    ///
    /// **The order of the gates is the whole point** (audit3 H6, carried to this lane): the
    /// authentication is cheapest and runs first, the throttle is charged on the ATTEMPT so a
    /// refusal is not free and repeatable, the byte allowance is reserved BEFORE the opener runs
    /// so a refused request never buys the disk read and the family arithmetic behind it, and the
    /// opener runs on a blocking thread rather than the runtime that also carries block relay,
    /// IBD and RPC.
    pub async fn resolve_interval_opening_for_serve(
        &self,
        peer: PeerKey,
        request: &PalwOpeningRequestV1<'_>,
    ) -> Result<Vec<u8>, PalwServeRefusalV1> {
        let interval_index = request.interval_index.ok_or(PalwServeRefusalV1::Malformed)?;
        self.authorize_serve(peer, request).await?;
        let claim = request.claim;
        {
            let mut served = self.openings.served_recently.lock().unwrap();
            let now = std::time::Instant::now();
            // Amortised and bounded, for the reason the material lane's map is (audit 2026-09-02):
            // sweeping on every request is a full walk under a lock any runtime worker can block
            // on. Here the key includes the peer, so the map is already bounded by the connection
            // count times the intervals one peer may ask for; the cap is the backstop.
            if served.len() >= SERVED_RECENTLY_CAP {
                served.retain(|_, at| now.duration_since(*at) < std::time::Duration::from_secs(60));
                if served.len() >= SERVED_RECENTLY_CAP {
                    served.clear();
                }
            }
            if let Some(at) = served.get(&(peer, claim, interval_index))
                && now.duration_since(*at) < INTERVAL_SERVE_THROTTLE
            {
                return Err(PalwServeRefusalV1::Throttled);
            }
            served.insert((peer, claim, interval_index), now);
        }
        let reservation = PALW_INTERVAL_OPENING_MAX_BYTES as u64;
        if !self.reserve_serve_budget(peer, reservation) {
            return Err(PalwServeRefusalV1::NoAllowance);
        }
        let resolver = { self.openings.resolver.lock().unwrap().clone() };
        let Some(resolver) = resolver else {
            self.refund_serve_budget(peer, reservation);
            return Err(PalwServeRefusalV1::NotHeld);
        };
        let opened = tokio::task::spawn_blocking(move || resolver(claim, interval_index)).await;
        let Some(bytes) = opened.ok().flatten() else {
            self.refund_serve_budget(peer, reservation);
            return Err(PalwServeRefusalV1::NotHeld);
        };
        if bytes.len() > PALW_INTERVAL_OPENING_MAX_BYTES {
            self.refund_serve_budget(peer, reservation); // never serve what the transport would refuse
            return Err(PalwServeRefusalV1::Oversized);
        }
        self.refund_serve_budget(peer, reservation.saturating_sub(bytes.len() as u64));
        Ok(bytes)
    }

    /// **Record that this node has asked the network for interval `interval_index` of `claim`.**
    ///
    /// Called by the seat just before it emits a `PalwIntervalOpeningRequest`. For the next
    /// [`PULL_SOLICITED_TTL`] an opening for that pair is admitted; before or after, it is
    /// [`PalwGossipAdmit::Unsolicited`]. A re-ask resets the window AND the slots: the seat is
    /// asking again because nothing it was served verified, so the payloads that filled the slots
    /// were not the answer.
    pub fn note_interval_pull_request(&self, claim: Hash64, interval_index: u32) {
        let mut pulls = self.openings.outstanding.lock().unwrap();
        let now = std::time::Instant::now();
        pulls.retain(|_, at| now.duration_since(*at) < PULL_SOLICITED_TTL);
        if pulls.len() >= OUTSTANDING_PULL_CAP && !pulls.contains_key(&(claim, interval_index)) {
            return;
        }
        pulls.insert((claim, interval_index), now);
        let mut slots = self.openings.slots.lock().unwrap();
        slots.retain(|key, _| pulls.contains_key(key));
        slots.remove(&(claim, interval_index));
    }

    fn is_interval_solicited(&self, claim: Hash64, interval_index: u32) -> bool {
        let pulls = self.openings.outstanding.lock().unwrap();
        pulls.get(&(claim, interval_index)).is_some_and(|at| std::time::Instant::now().duration_since(*at) < PULL_SOLICITED_TTL)
    }

    /// Give back a slot whose payload never reached the seat — the eviction bookkeeping of
    /// [`Self::forget_digest`], for this lane's counters.
    fn release_opening_slot(&self, peer: PeerKey, claim: Hash64, interval_index: u32) {
        let mut slots = self.openings.slots.lock().unwrap();
        let Some(slot) = slots.get_mut(&(claim, interval_index)) else { return };
        slot.admitted = slot.admitted.saturating_sub(1);
        if let Some(count) = slot.per_peer.get_mut(&peer) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                slot.per_peer.remove(&peer);
            }
        }
    }

    fn digest_opening(&self, claim: &Hash64, interval_index: u32, bytes: &[u8]) -> u64 {
        let mut h = self.hasher.build_hasher();
        3u8.hash(&mut h);
        claim.as_byte_slice().hash(&mut h);
        interval_index.hash(&mut h);
        bytes.hash(&mut h);
        h.finish()
    }

    /// Admit an interval opening served to this node. `Fresh` means: push
    /// [`PalwGossipEvent::IntervalOpening`] to the inbox (done here). Never relayed, whatever the
    /// verdict — the lane is asker-specific.
    ///
    /// **The size cap is checked before anything else touches the bytes**, which is what "bounded
    /// before deserialising" means: nothing downstream of here sees a payload this node did not
    /// first agree to hold.
    ///
    /// Admitted only for a pair this node asked for and is still waiting on: an unasked opening
    /// has no consumer, so accepting one would let any peer fill the inbox for the cost of a claim
    /// id read off a header. Within a solicited pair the material lane's two bounds apply per
    /// interval — a slot ceiling, and a per-peer share of it that leaves room for the executor's
    /// answer however fast a flooder sends.
    pub fn admit_interval_opening(&self, peer: PeerKey, claim: Hash64, interval_index: u32, bytes: &[u8]) -> PalwGossipAdmit {
        if bytes.len() > PALW_INTERVAL_OPENING_MAX_BYTES {
            return PalwGossipAdmit::TooBig;
        }
        if !self.is_interval_solicited(claim, interval_index) {
            return PalwGossipAdmit::Unsolicited;
        }
        let digest = self.digest_opening(&claim, interval_index, bytes);
        // **The digest goes first, and the slot is spent only by bytes that were new.** The other
        // order looks equivalent and is not: a peer that repeats the SAME payload would pass the
        // slot check, spend a slot, and only then be told it is a duplicate — so four repeats of
        // one message exhaust an interval's slots and the honest executor's answer is refused
        // afterwards, which is the flood the slots exist to stop, performed by the slots.
        //
        // Outside the slot lock, too: `admit_digest` takes the center's own state lock, and a lane
        // lock held across it would order two locks that nothing else orders.
        if self.admit_digest(digest, None, true) != PalwGossipAdmit::Fresh {
            return PalwGossipAdmit::Duplicate;
        }
        {
            let mut slots = self.openings.slots.lock().unwrap();
            let slot = slots.entry((claim, interval_index)).or_default();
            let from_peer = slot.per_peer.get(&peer).copied().unwrap_or(0);
            if from_peer >= PALW_OPENINGS_PER_PEER_PER_INTERVAL || slot.admitted >= PALW_OPENINGS_PER_INTERVAL {
                drop(slots);
                // Give the digest back with the slot we did not take: a payload this node refused
                // to hold must not be remembered as one it has seen, or the honest re-send of the
                // same bytes after a re-ask is a `Duplicate` for the next `SEEN_CAP` messages.
                self.forget_digest(digest);
                return PalwGossipAdmit::Duplicate;
            }
            slot.admitted += 1;
            *slot.per_peer.entry(peer).or_insert(0) += 1;
        }
        if self.has_consumer()
            && self.inbox_tx.try_send(PalwGossipEvent::IntervalOpening { claim, interval_index, bytes: bytes.to_vec() }).is_err()
        {
            self.release_opening_slot(peer, claim, interval_index);
            // **A payload the inbox refused must not leave its digest marked seen** (audit
            // 2026-09-02, the same fault on this lane): the seat re-asks for exactly this pair,
            // the honest executor answers with the identical bytes, and a remembered digest would
            // answer `Duplicate` for the next SEEN_CAP messages — a seat permanently unable to
            // verify a claim it is seated on.
            self.forget_digest(digest);
            return PalwGossipAdmit::Duplicate;
        }
        PalwGossipAdmit::Fresh
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
    /// **A peer's relay budget is in bytes, and the claim id cannot move it** (mainnet audit,
    /// 2026-09-05). Every earlier bound is keyed by a claim id the peer chooses; a fresh id per
    /// payload sat under all of them and had each 16 MiB payload relayed to every peer. Four maximal
    /// unsolicited materials fit the window; the fifth is dropped, and a solicited answer still
    /// goes through.
    #[test]
    fn an_unsolicited_peer_cannot_relay_more_bytes_than_its_window_budget_whatever_it_calls_the_claim() {
        let center = PalwGossipCenter::default();
        let body = vec![0u8; PALW_MATERIAL_MAX_BYTES];
        assert_eq!(PALW_MATERIAL_BYTES_PER_PEER_PER_WINDOW, 4 * PALW_MATERIAL_MAX_BYTES as u64);
        for i in 1..=4u64 {
            assert_eq!(
                center.admit_material(peer(1), Hash64::from_u64_word(0x1000 + i), &body),
                PalwGossipAdmit::Fresh,
                "material {i} is inside the budget"
            );
        }
        assert_eq!(
            center.admit_material(peer(1), Hash64::from_u64_word(0x1005), &body),
            PalwGossipAdmit::Duplicate,
            "the fifth maximal material from the same peer is over budget, fresh claim id or not"
        );
        assert_eq!(
            center.admit_material(peer(1), Hash64::from_u64_word(0x1006), b"tiny"),
            PalwGossipAdmit::Duplicate,
            "…and so is anything else in the window"
        );
        // Another peer has its own window.
        assert_eq!(center.admit_material(peer(2), Hash64::from_u64_word(0x1007), &body), PalwGossipAdmit::Fresh);
        // An answer this node asked for is exempt from the budget (still deduplicated and capped per claim).
        let asked = Hash64::from_u64_word(0x1008);
        center.note_pull_request(asked);
        assert_eq!(
            center.admit_material(peer(1), asked, &body),
            PalwGossipAdmit::Fresh,
            "a solicited answer is not crowded out by the budget"
        );
    }

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

    /// A free-prompt material declaring the privacy mode `mode`, with no ids (a `PanelDa`
    /// commitment carries none) — junk to a seat, exactly the bytes the transport must judge.
    fn material_in_mode(mode: u8) -> Vec<u8> {
        use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3};
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(9),
            class_id: Hash64::from_u64_word(7),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint {
                transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(1),
                index: 0,
            },
            executor_pubkey: vec![7; 8],
            operator_id: Hash64::from_u64_word(4),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 100,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::from_u64_word(0x71),
            prompt_tokens: 3,
            decode_token_limit: 3,
            max_context_tokens: 16,
            privacy_mode: mode,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        kaspa_consensus_core::palw_freeprompt_v3::palw_fp_material_encode_v1(&job, &[])
    }

    /// **A `PanelDa` material is never relayed, and reaches only the node that asked for it**
    /// (ADR-0077 Decision 16; private-prompts design, 2026-09-05).
    ///
    /// The prompt inside a mode-2 material is what the commitment withheld from the chain. Every
    /// other material is relayed to the mesh on first sighting; this one must answer `Private`
    /// whoever sends it, must leave no digest behind when it was not asked for (so the honest
    /// pull's answer is not a `Duplicate` of a dropped stranger's copy), and must be handed to
    /// the inbox only when this node solicited it. The public material beside it is untouched.
    #[test]
    fn a_panel_da_material_stops_at_the_node_that_asked_for_it() {
        use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA};
        let center = PalwGossipCenter::default();
        let mut rx = center.take_inbox().expect("the first taker gets it");
        let claim = Hash64::from_u64_word(7);
        let private = material_in_mode(PALW_FP_PRIVACY_PANEL_DA);
        let public = material_in_mode(PALW_FP_PRIVACY_PUBLIC_DA);
        assert!(material_is_private(&private));
        assert!(!material_is_private(&public));
        assert!(!material_is_private(b"not a material"));

        // Unasked: refused, not relayed, not queued, and no digest written.
        assert_eq!(center.admit_material(peer(1), claim, &private), PalwGossipAdmit::Private);
        assert!(rx.try_recv().is_err(), "an unasked private material never reaches the consumer");

        // Asked: kept for this node's own seat, still never relayed.
        center.note_pull_request(claim);
        assert_eq!(center.admit_material(peer(2), claim, &private), PalwGossipAdmit::Private, "solicited is still not Fresh");
        match rx.try_recv() {
            Ok(PalwGossipEvent::Material { claim: got, bytes }) => {
                assert_eq!(got, claim);
                assert_eq!(bytes, private);
            }
            other => panic!("the solicited private material must reach the inbox: {other:?}"),
        }
        assert_eq!(center.admit_material(peer(2), claim, &private), PalwGossipAdmit::Private, "a re-sighting is not relayed either");

        // The public material beside it is the ordinary lane, unchanged.
        assert_eq!(center.admit_material(peer(1), Hash64::from_u64_word(8), &public), PalwGossipAdmit::Fresh);
    }

    /// **An unsigned pull is a stranger's, and a private material is not served on it** — the
    /// bytes are read and the window is charged exactly as for a served answer, so the refusal
    /// cannot be probed for free; the same material on the signed lane without an authorizer
    /// installed is `NotAReader`, because a node that cannot ask who is asking cannot say yes.
    #[tokio::test]
    async fn a_panel_da_material_is_not_served_to_a_stranger() {
        use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA};
        let center = PalwGossipCenter::default();
        let private = material_in_mode(PALW_FP_PRIVACY_PANEL_DA);
        let public = material_in_mode(PALW_FP_PRIVACY_PUBLIC_DA);
        let (private_claim, public_claim, other_private_claim) =
            (Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3));
        let served = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let counted = served.clone();
        let (p, q) = (private.clone(), public.clone());
        center.set_material_resolver(std::sync::Arc::new(move |claim| {
            *counted.lock().unwrap() += 1;
            if claim == public_claim { Some(q.clone()) } else { Some(p.clone()) }
        }));
        assert_eq!(center.resolve_material_for_serve(peer(1), private_claim).await, None, "unsigned: not served");
        assert_eq!(*served.lock().unwrap(), 1, "…and the read was made, so the refusal was priced");
        assert_eq!(center.resolve_material_for_serve(peer(1), public_claim).await, Some(public), "the public lane is unchanged");

        // A claim this node has not just served: the per-claim serve throttle comes BEFORE the
        // read, so a re-ask inside it is `NotHeld` whatever the material is — that is the
        // throttle's own test; this one is about the reader rule.
        let request = PalwOpeningRequestV1 {
            claim: other_private_claim,
            interval_index: None,
            requested_daa: 0,
            requester_pubkey: &[],
            signature: &[],
        };
        assert_eq!(
            center.resolve_material_for_serve_signed(peer(2), &request).await,
            Err(PalwServeRefusalV1::NotAReader),
            "no authorizer installed: nobody can be a reader"
        );
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

        // **A serve that reached the resolver and produced nothing is still charged.** This is
        // audit3 H6's actual property: the old code recorded `served_recently` only on SUCCESS, so
        // a request for a claim this node does not hold was never throttled and could be repeated
        // immediately and indefinitely, with zero outbound traffic to make it visible. The record
        // is written before the read, so every path past the reservation carries it.
        let absent = h64(13);
        center.set_material_resolver(std::sync::Arc::new(move |_| None));
        assert!(center.resolve_material_for_serve(peer(4), absent).await.is_none(), "nothing on disk, nothing served");
        assert!(
            center.served_recently.lock().unwrap().recorded_at(&absent).is_some(),
            "a serve that found nothing still costs the asker its throttle window, or the refusal is free and repeatable"
        );

        // **The budget refusal, which is the one path that is NOT charged — and must not be**
        // (audit 2026-09-02 round 3). Exhaust a peer's allowance and ask it for a claim nothing has
        // touched yet.
        let other = h64(12);
        while center.reserve_serve_budget(peer(3), PALW_MATERIAL_MAX_BYTES as u64) {}
        assert!(center.resolve_material_for_serve(peer(3), other).await.is_none(), "no allowance, no serve");

        // (a) The disk was never touched. The old order was read-then-budget, so an 80-byte request
        //     bought a full synchronous read of up to 16 MiB that produced no bytes at all.
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1, "the allowance is consulted BEFORE the disk");
        // (b) And it left NO record. Charging the window to a request the budget refused is what
        //     made the map flushable for free: a peer whose share is spent costs this node two
        //     mutex acquisitions per message, and it was buying an eviction with each of them.
        //     `SERVED_RECENTLY_CAP` of those walk every honest claim's record out of the map. A
        //     record now costs `SERVE_ATTEMPT_FLOOR_BYTES` of the asker's own share, which is the
        //     only currency nothing on the wire can reset.
        assert!(
            center.served_recently.lock().unwrap().recorded_at(&other).is_none(),
            "a request this node's budget refused must not buy an entry in the map — that is the eviction being free"
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
                PalwGossipEvent::IntervalOpening { .. } => panic!("nothing on the interval lane was admitted here"),
            }
        }
        assert_eq!((materials, receipts), (5, 1), "4 for claim 7, 1 for claim 8; the one receipt — marks push nothing");
    }

    // --- the interval lane (ADR-0077 Decision 8 / SA-2, ADR-0079 SA-3) ---

    /// A well-shaped key and signature — the lengths ML-DSA-87 actually produces, which is what
    /// `check_opening_request_shape` demands before anything else runs.
    fn signed_request() -> (Vec<u8>, Vec<u8>) {
        (vec![0xAAu8; 2_592], vec![0xBBu8; 4_627])
    }

    /// A stand-in for the panel service's authorizer: it maps ONE key to a bond and refuses every
    /// other, which is what the chain does with `palw_bond_of_pubkey_v2`.
    fn one_bond_authorizer(good_key: Vec<u8>, bond: Hash64) -> PalwOpeningAuthorizer {
        std::sync::Arc::new(move |req: &PalwOpeningRequestV1<'_>| {
            if req.requester_pubkey != good_key.as_slice() {
                return Err(PalwServeRefusalV1::NotBonded);
            }
            if req.signature.iter().all(|b| *b == 0) {
                return Err(PalwServeRefusalV1::BadSignature);
            }
            if req.requested_daa == 0 {
                return Err(PalwServeRefusalV1::Stale);
            }
            Ok(bond)
        })
    }

    /// **The shape check runs before anything is read** (ADR-0079 Decision 4's
    /// `check_opening_request_shape`), and every refusal has a name.
    ///
    /// The names are the point: a seat that is REFUSED and a seat that hears NOTHING look
    /// identical from the seat's side, and only one of them is the executor's fault. An operator
    /// reading this node's log must be able to tell which happened without a packet capture.
    #[test]
    fn an_unsigned_or_misshapen_request_is_refused_by_name_before_any_work() {
        let claim = h64(101);
        let (key, sig) = signed_request();

        let unsigned =
            PalwOpeningRequestV1 { claim, interval_index: Some(0), requested_daa: 10, requester_pubkey: &[], signature: &[] };
        assert_eq!(check_opening_request_shape(&unsigned), Err(PalwServeRefusalV1::Unsigned));
        assert_eq!(PalwServeRefusalV1::Unsigned.name(), "unsigned");

        let short_key = vec![1u8; 32];
        let stubby = PalwOpeningRequestV1 { requester_pubkey: &short_key, signature: &sig, ..unsigned };
        assert_eq!(check_opening_request_shape(&stubby), Err(PalwServeRefusalV1::Malformed));

        let short_sig = vec![1u8; 64];
        let stubby_sig = PalwOpeningRequestV1 { requester_pubkey: &key, signature: &short_sig, ..unsigned };
        assert_eq!(check_opening_request_shape(&stubby_sig), Err(PalwServeRefusalV1::Malformed));

        let whole = PalwOpeningRequestV1 { requester_pubkey: &key, signature: &sig, ..unsigned };
        assert_eq!(check_opening_request_shape(&whole), Ok(()));
    }

    /// **A stranger, a forged signature and a stale request are all refused, and each by its own
    /// name** (ADR-0077 SA-2). The center holds no key: the authorizer is the panel service, the
    /// only party here that holds one — ADR-0079 Decision 4's rule that a process parsing a
    /// stranger's bytes holds no key, stated as a type.
    #[tokio::test]
    async fn only_a_bonded_requester_is_authorized_and_the_rate_is_per_bond() {
        let center = PalwGossipCenter::default();
        let claim = h64(102);
        let (key, sig) = signed_request();
        let bond = h64(0xB0);
        center.set_opening_authorizer(one_bond_authorizer(key.clone(), bond));

        let ok = PalwOpeningRequestV1 { claim, interval_index: Some(3), requested_daa: 900, requester_pubkey: &key, signature: &sig };
        assert_eq!(center.authorize_serve(peer(1), &ok).await, Ok(bond));

        let stranger_key = vec![0xCCu8; 2_592];
        let stranger = PalwOpeningRequestV1 { requester_pubkey: &stranger_key, ..ok };
        assert_eq!(center.authorize_serve(peer(1), &stranger).await, Err(PalwServeRefusalV1::NotBonded));

        let zero_sig = vec![0u8; 4_627];
        let forged = PalwOpeningRequestV1 { signature: &zero_sig, ..ok };
        assert_eq!(center.authorize_serve(peer(1), &forged).await, Err(PalwServeRefusalV1::BadSignature));

        let stale = PalwOpeningRequestV1 { requested_daa: 0, ..ok };
        assert_eq!(center.authorize_serve(peer(1), &stale).await, Err(PalwServeRefusalV1::Stale));

        // The per-bond rate. Only requests the authorizer already mapped to a bond are charged, so
        // an unbonded flooder never reaches this counter at all — which is why the counter can be
        // a plain map: its keys are collateral somebody posted.
        for _ in 1..OPENING_REQUESTS_PER_BOND_PER_WINDOW {
            assert_eq!(center.authorize_serve(peer(1), &ok).await, Ok(bond));
        }
        assert_eq!(center.authorize_serve(peer(1), &ok).await, Err(PalwServeRefusalV1::RateLimited), "the bond spent its window");
    }

    /// **A node with no authorizer refuses every opening request** — it has no capture to open
    /// either, so this is the honest silence of a node with no PALW role and not a policy.
    #[tokio::test]
    async fn a_node_that_serves_nothing_authorizes_nobody() {
        let center = PalwGossipCenter::default();
        assert!(!center.serves_openings());
        let claim = h64(103);
        let (key, sig) = signed_request();
        let req = PalwOpeningRequestV1 { claim, interval_index: Some(0), requested_daa: 5, requester_pubkey: &key, signature: &sig };
        assert_eq!(center.resolve_interval_opening_for_serve(peer(1), &req).await, Err(PalwServeRefusalV1::NotBonded));
    }

    /// **An opening serve is authenticated, throttled and budgeted, in that order** (ADR-0077
    /// SA-2 over audit3 H5/H6/H10): the authentication is cheapest and runs first, a refusal
    /// charges the asker's throttle window so it is not free and repeatable, the byte allowance is
    /// consulted BEFORE the opener runs, and the throttle is per ASKER so two seats that drew the
    /// same interval are both answered.
    #[tokio::test]
    async fn an_opening_serve_is_authenticated_throttled_per_asker_and_budgeted_before_the_opener_runs() {
        let center = PalwGossipCenter::default();
        let claim = h64(104);
        let (key, sig) = signed_request();
        center.set_opening_authorizer(one_bond_authorizer(key.clone(), h64(0xB1)));
        let opens = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = opens.clone();
        center.set_interval_opening_resolver(std::sync::Arc::new(move |_, index| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(vec![index as u8; 512])
        }));
        assert!(center.serves_openings());

        let ask = |interval: u32| PalwOpeningRequestV1 {
            claim,
            interval_index: Some(interval),
            requested_daa: 900,
            requester_pubkey: &key,
            signature: &sig,
        };

        // An unsigned request never reaches the opener.
        let unsigned = PalwOpeningRequestV1 { requester_pubkey: &[], signature: &[], ..ask(7) };
        assert_eq!(center.resolve_interval_opening_for_serve(peer(1), &unsigned).await, Err(PalwServeRefusalV1::Unsigned));
        assert_eq!(opens.load(std::sync::atomic::Ordering::SeqCst), 0, "an unsigned request bought no disk read");

        assert_eq!(center.resolve_interval_opening_for_serve(peer(1), &ask(7)).await, Ok(vec![7u8; 512]));
        assert_eq!(opens.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            center.resolve_interval_opening_for_serve(peer(1), &ask(7)).await,
            Err(PalwServeRefusalV1::Throttled),
            "the same asker is throttled"
        );
        assert_eq!(opens.load(std::sync::atomic::Ordering::SeqCst), 1, "and the refusal did not run the opener");
        assert!(
            center.resolve_interval_opening_for_serve(peer(2), &ask(7)).await.is_ok(),
            "a second seat that drew the same interval is answered — the throttle is per asker"
        );
        assert!(center.resolve_interval_opening_for_serve(peer(1), &ask(8)).await.is_ok(), "another interval is another ask");

        // Out of allowance: the opener must not run.
        while center.reserve_serve_budget(peer(3), PALW_INTERVAL_OPENING_MAX_BYTES as u64) {}
        let before = opens.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            center.resolve_interval_opening_for_serve(peer(3), &ask(9)).await,
            Err(PalwServeRefusalV1::NoAllowance),
            "no allowance, no serve"
        );
        assert_eq!(opens.load(std::sync::atomic::Ordering::SeqCst), before, "the allowance is consulted BEFORE the opener");
        assert!(
            center.openings.served_recently.lock().unwrap().contains_key(&(peer(3), claim, 9)),
            "a refused serve still costs the asker its throttle window"
        );

        // An opener that outgrows the cap is refused rather than emitted: the far end would drop
        // it, and a serve the other end cannot take only spends this node's egress.
        center.set_interval_opening_resolver(std::sync::Arc::new(|_, _| Some(vec![0u8; PALW_INTERVAL_OPENING_MAX_BYTES + 1])));
        assert_eq!(center.resolve_interval_opening_for_serve(peer(5), &ask(1)).await, Err(PalwServeRefusalV1::Oversized));
    }

    /// **An opening is admitted only because this node asked for exactly that interval, and only
    /// within its slots** (ADR-0077 Decision 8).
    ///
    /// The lane is asker-specific: an opening nobody asked for has no consumer, so admitting one
    /// would let any peer queue megabytes against a claim id read off a header. A solicited pair
    /// still has the material lane's two bounds — a slot ceiling and a per-peer share of it — so a
    /// forger cannot make the seat attempt unbounded replays, and cannot be the only voice for an
    /// interval however fast it sends.
    #[test]
    fn an_interval_opening_is_admitted_only_when_solicited_and_within_its_slots() {
        let center = PalwGossipCenter::default();
        let mut rx = center.take_inbox().expect("first taker");
        let claim = h64(105);

        assert_eq!(center.admit_interval_opening(peer(1), claim, 5, b"early"), PalwGossipAdmit::Unsolicited, "not asked for");
        assert!(rx.try_recv().is_err(), "an unsolicited opening reaches no consumer");

        center.note_interval_pull_request(claim, 5);
        assert_eq!(center.admit_interval_opening(peer(1), claim, 6, b"wrong-interval"), PalwGossipAdmit::Unsolicited);
        assert_eq!(center.admit_interval_opening(peer(1), claim, 5, b"answer"), PalwGossipAdmit::Fresh);
        assert_eq!(center.admit_interval_opening(peer(1), claim, 5, b"answer"), PalwGossipAdmit::Duplicate, "same bytes twice");
        assert_eq!(
            center.admit_interval_opening(peer(1), claim, 5, b"junk-1"),
            PalwGossipAdmit::Fresh,
            "the repeat above spent no slot — four repeats of one message must not exhaust the interval"
        );
        assert_eq!(
            center.admit_interval_opening(peer(1), claim, 5, b"junk-2"),
            PalwGossipAdmit::Duplicate,
            "one peer holds at most its couple of slots"
        );
        assert_eq!(center.admit_interval_opening(peer(2), claim, 5, b"junk-2"), PalwGossipAdmit::Fresh, "another peer has its own");
        assert_eq!(center.admit_interval_opening(peer(2), claim, 5, b"junk-3"), PalwGossipAdmit::Fresh, "its second");
        assert_eq!(center.admit_interval_opening(peer(3), claim, 5, b"junk-4"), PalwGossipAdmit::Duplicate, "the slots are spent");
        assert_eq!(
            center.admit_interval_opening(peer(3), claim, 5, &vec![0u8; PALW_INTERVAL_OPENING_MAX_BYTES + 1]),
            PalwGossipAdmit::TooBig
        );

        let mut delivered = 0usize;
        while let Ok(event) = rx.try_recv() {
            match event {
                PalwGossipEvent::IntervalOpening { claim: c, interval_index, .. } => {
                    assert_eq!((c, interval_index), (claim, 5));
                    delivered += 1;
                }
                other => panic!("the interval lane delivered {other:?}"),
            }
        }
        assert_eq!(delivered, PALW_OPENINGS_PER_INTERVAL, "exactly the admitted payloads reached the seat");

        // A re-ask is the seat saying nothing it was served verified: the slots are cleared so the
        // honest answer can still get in.
        center.note_interval_pull_request(claim, 5);
        assert_eq!(
            center.admit_interval_opening(peer(4), claim, 5, b"late-honest"),
            PalwGossipAdmit::Fresh,
            "a re-ask reopens the slots"
        );
    }

    /// **The interval lane sits above the material pull in the version ladder, and its cap below
    /// the whole-capture cap.** A peer that can be asked for an opening can also be asked for the
    /// material — the court's close still needs whole bytes — and an "opening" can never be the
    /// capture under another name, which is the transport half of W10.
    #[test]
    fn the_interval_lane_is_gated_above_the_pull_and_capped_below_the_capture() {
        use crate::flow_context::{PROTOCOL_VERSION_PALW_INTERVAL, PROTOCOL_VERSION_PALW_PULL};
        assert!(PROTOCOL_VERSION_PALW_INTERVAL > PROTOCOL_VERSION_PALW_PULL);
        assert!(PALW_INTERVAL_OPENING_MAX_BYTES < PALW_MATERIAL_MAX_BYTES);
    }

    /// **A stranger must not be able to switch the serve throttle off for everybody else** (audit
    /// 2026-09-02, and its round-3 re-check).
    ///
    /// The first bound swept at the cap and then called `clear()`, so ~287 KB of ~70-byte requests
    /// deleted the throttle record of every honest claim at once. Per-entry, oldest-first eviction
    /// replaced it — and a flood of exactly `SERVED_RECENTLY_CAP` fresh ids walks the identical
    /// honest record out of the map one entry at a time, for the identical price. The first
    /// version of this test flooded `CAP - 1` after recording the honest claim as the NEWEST
    /// entry, which is the off-by-one that made it pass: it asserted a property the code did not
    /// have.
    ///
    /// What makes the map unfloodable is not the eviction policy at all — it is that a record is
    /// written only by a request the per-peer budget ADMITTED, so an eviction costs
    /// `SERVE_ATTEMPT_FLOOR_BYTES` and no message can reset that. One peer's whole share buys
    /// ~513 records; filling the map takes 4096. This floods twice the cap from one peer and
    /// demands both halves: the map does not fill, and the honest record is still there.
    #[tokio::test]
    async fn the_serve_throttle_is_not_a_map_a_stranger_can_flush() {
        let center = PalwGossipCenter::default();
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = reads.clone();
        center.set_material_resolver(std::sync::Arc::new(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(vec![0u8; 1024])
        }));

        // The honest seat's pull is answered once, and is the OLDEST entry in the map from here on
        // — the position a FIFO eviction reaches first, which is the case worth asserting.
        let honest = h64(0xA11CE);
        assert!(center.resolve_material_for_serve(peer(1), honest).await.is_some(), "the honest seat's pull is answered once");

        // A stranger names a fresh claim per ~70-byte request — twice as many as the map can hold,
        // which is twice what walking every entry out of it would take if the inserts were free.
        for n in 0..(SERVED_RECENTLY_CAP as u64 * 2) {
            let _ = center.resolve_material_for_serve(peer(2), h64(1_000_000 + n)).await;
        }

        // Its share bought it ~513 records and no more, so the map never came near the cap it
        // would have to reach to evict anybody.
        let ceiling = (SERVE_BUDGET_BYTES_PER_PEER / SERVE_ATTEMPT_FLOOR_BYTES) as usize;
        let held = center.served_recently.lock().unwrap().len();
        assert!(
            held <= ceiling + 1,
            "one peer put {held} entries in the throttle map against its own {ceiling}-attempt share — the record is not being \
             charged to a request the budget admitted"
        );
        assert!(
            held < SERVED_RECENTLY_CAP,
            "one peer filled the throttle map ({held} of {SERVED_RECENTLY_CAP}), which is what evicting somebody else's record takes"
        );

        // The honest claim is inside its window still, and no message any peer can send may change
        // that. Asked from a THIRD peer, so this is the claim throttle and not a spent budget.
        // (The flooder's own admitted requests did read the disk — that is the floor being charged
        // — so the count is sampled here, and what must not move is the read for SOMEBODY ELSE'S
        // claim.)
        let reads_before = reads.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            center.resolve_material_for_serve(peer(3), honest).await.is_none(),
            "a flooder must not be able to buy this node a fresh read of somebody else's claim"
        );
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), reads_before, "and the disk was not touched for it");
    }

    /// **Nor by every peer key it can afford at once** (audit 2026-09-02 round 3, the throttle
    /// half).
    ///
    /// The test above is one peer against its own 48 MiB share, and a re-check could fairly say
    /// that bounds one peer and not the attack: the flooder controls how many peer keys it opens.
    /// The answer is the node-wide ceiling, and it is arithmetic rather than judgement. A record
    /// is written only by a request the budget ADMITTED, admission needs the 16 MiB worst-case
    /// reservation to fit inside what is left of the node's 256 MiB minute, so the node admits at
    /// most (256 − 16) MiB / 64 KiB = 3841 attempts in a window — fewer than the map's 4096
    /// entries. Filling it inside one window is not expensive, it is impossible, at any number of
    /// peer keys.
    ///
    /// What the eviction cost when the record was charged before the budget: 4096 ~70-byte
    /// requests, from one peer, needing no allowance at all — which is what this floods, and it
    /// is red on that code.
    #[tokio::test]
    async fn the_serve_throttle_cannot_be_flushed_by_sybil_peers_either() {
        let center = PalwGossipCenter::default();
        center.set_material_resolver(std::sync::Arc::new(move |_| Some(vec![0u8; 1024])));

        // The honest seat's pull, recorded first and therefore the FIFO's next victim.
        let honest = h64(0xA11CE);
        assert!(center.resolve_material_for_serve(peer(1), honest).await.is_some(), "the honest seat's pull is answered once");

        // Eight peer keys — one more than the node-wide ceiling divided by a peer's own share, so
        // the node's allowance is what runs out first and not any peer's — each naming twice the
        // map's whole capacity in fresh ids. 8192 requests, against a map of 4096.
        for p in 2..10u128 {
            for n in 0..(SERVED_RECENTLY_CAP as u64) {
                let _ = center.resolve_material_for_serve(peer(p), h64(1_000_000 * p as u64 + n)).await;
            }
        }

        let node_wide = ((SERVE_BUDGET_BYTES_PER_WINDOW - PALW_MATERIAL_MAX_BYTES as u64) / SERVE_ATTEMPT_FLOOR_BYTES + 1) as usize;
        let held = center.served_recently.lock().unwrap().len();
        assert!(
            held <= node_wide + 1,
            "{held} entries against a node-wide ceiling of {node_wide} admitted attempts a window — the record is not being \
             charged to a request the budget admitted"
        );
        assert!(
            held < SERVED_RECENTLY_CAP,
            "eight peer keys filled the throttle map ({held} of {SERVED_RECENTLY_CAP}); the honest record is evictable for the \
             price of ~70-byte messages again"
        );
        assert!(
            center.served_recently.lock().unwrap().recorded_at(&honest).is_some(),
            "and the honest claim's window, which is the thing the flood was buying, is still there"
        );

        // **And the flush would have bought nothing anyway** — this is the bound, not the margin
        // above. The 3841 admissions it takes to reach the cap ARE the node's serve allowance, so
        // a flooder that spent them cannot buy the read the evicted record was refusing either.
        assert!(
            !center.reserve_serve_budget(peer(99), PALW_MATERIAL_MAX_BYTES as u64),
            "a node whose throttle map has been flooded has, by that very fact, no allowance left to serve anybody"
        );
    }

    /// **A serve attempt costs its asker a floor, whatever the attempt returns** (audit
    /// 2026-09-02).
    ///
    /// The per-peer share bounded BYTES: the 16 MiB reservation was refunded down to the material's
    /// real size, and refunded in full on every path that produced none. So a 1 KiB material left
    /// one peer's 48 MiB good for tens of thousands of synchronous disk reads a minute, and a
    /// request for a claim this node does not hold was free. What a request actually costs this
    /// node is a blocking-pool thread and a `std::fs::read` — a per-REQUEST cost — so the ceiling
    /// has to be per request too. This is the bound that still holds when the claim throttle has
    /// been defeated, because it is keyed by `PeerKey` and nothing on the wire can flush it.
    #[tokio::test]
    async fn a_serve_attempt_costs_its_asker_a_floor_whatever_it_returns() {
        let center = PalwGossipCenter::default();
        let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = reads.clone();
        center.set_material_resolver(std::sync::Arc::new(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(vec![0u8; 1024]) // a small material: the case the byte ceiling never bounded
        }));

        let attempts = 1500u64;
        let mut served = 0usize;
        for n in 0..attempts {
            if center.resolve_material_for_serve(peer(1), h64(2_000_000 + n)).await.is_some() {
                served += 1;
            }
        }
        let ceiling = (SERVE_BUDGET_BYTES_PER_PEER / SERVE_ATTEMPT_FLOOR_BYTES) as usize;
        assert!(
            served <= ceiling,
            "one peer forced {served} reads of a 1 KiB material against a {ceiling}-attempt ceiling — the share bounds bytes, \
             and the cost of a request is not its bytes"
        );
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), served, "every read that happened produced a serve");
        assert!(served > 0, "and an honest peer is still answered — the floor is a ceiling on volume, not a refusal");
    }

    /// **A producer that could not queue its own capture is not blind to it forever** (audit
    /// 2026-09-02).
    ///
    /// `mark_own_material` recorded the digest and then dropped the payload on a full inbox — and
    /// unlike the remote path there is no way back: the echo a peer relays is the identical bytes,
    /// so it hashes to the identical digest and comes back `Duplicate`; a `PalwMaterialRequest`
    /// pull returns the same bytes with the same result; and the execution is deterministic, so
    /// re-running and re-marking is a no-op. The function's own doc block names what that costs —
    /// "a producer whose own capture never reached its own panel service had nothing to answer
    /// with, and the session ran out", i.e. a dispute is opened against the executor and the
    /// executor is defaulted.
    #[test]
    fn a_producer_whose_inbox_was_full_can_still_receive_its_own_capture() {
        let center = PalwGossipCenter::default();
        let mut rx = center.take_inbox().expect("first taker");
        for n in 0..INBOX_CAP as u64 {
            assert_eq!(center.admit_material(peer(1), h64(n), b"payload"), PalwGossipAdmit::Fresh, "event {n} is queued");
        }

        let claim = h64(9_002);
        let capture = b"the producer's own execution capture";
        center.mark_own_material(claim, capture);

        // The panel service catches up, and the echo a peer relays back is the only copy left.
        let mut drained = 0usize;
        while rx.try_recv().is_ok() {
            drained += 1;
        }
        assert_eq!(drained, INBOX_CAP, "the inbox was full, which is the precondition this test needs");

        assert_eq!(
            center.admit_material(peer(2), claim, capture),
            PalwGossipAdmit::Fresh,
            "the executor's own bytes must still be admissible, or it has nothing to answer a court with"
        );
        match rx.try_recv() {
            Ok(PalwGossipEvent::Material { claim: got, .. }) => assert_eq!(got, claim, "and its own panel finally holds it"),
            other => panic!("the echo must reach the producer's own service, got {other:?}"),
        }
    }
}
