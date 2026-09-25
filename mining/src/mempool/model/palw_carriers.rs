//! **ADR-0152 v3.1 H-1 (P2-9): the pool's lifecycle carriers** — the index the template's carrier
//! lane reads, the lane's bounds, and the mempool reserve that keeps a carrier in a full pool.
//!
//! H-1 obliges a heartbeat miner to include the conviction, DA, reporter and court objects filed
//! during a licence halt, when heartbeats may be the only blocks (V-8). Three node rules serve it,
//! all gated by `Config::palw_h1_carrier_priority` (testnet-12 only):
//!
//! * **The lane** (`TransactionsPool::build_palw_carrier_lane`): the ready carriers lead every
//!   template, up to half its mass ([`PALW_H1_CARRIER_LANE_MASS_DIVISOR`]), in the order this index
//!   keeps — feerate descending, then ARRIVAL — one per [`PalwH1LaneKeyV1`].
//! * **The reserve** ([`PalwCarrierReserveV1`]): up to a bounded count and bytes of carriers, one
//!   per lane key, hold a reserved place — such a carrier takes a full pool's room from the
//!   cheapest unreserved transactions whatever they pay, and nothing evicts it.
//! * **The gate** (consensus: `palw_mempool_h1_carrier_refusal`): only a carrier the tip's fold would
//!   take enters the pool at all, and one it stops taking is evicted at the next template. That is
//!   what makes the other two safe to give: the lane and the reserve are sold to objects the fold
//!   folds, not to anything that decodes (P2-9 review, finding 5).
//!
//! **A possession proof whose row is about to lapse leads the lane** (the 2026-09-25
//! model-registry review, M1; `palw_readiness_escalation_v1`). Past R-core+ the gate puts every
//! proof to the fold and the index keeps every proof; the tip read (`ConsensusApi::
//! palw_readiness_urgency_v1`, asked when a proof enters and at every new block) says how urgently
//! its `(bond, class)` row needs it. Per row, ONE copy is the row's representative — the best
//! (feerate, then arrival) copy that escalates, is READY (no parent in the pool) and fits the lane —
//! and only the representative is a carrier (the M1 review, HIGH 1: every escalating copy used to be
//! one, so 1,025 copies of one row, over the lane's budget or chained on a pool parent, filled the
//! scans and emptied the lane). The representatives are the candidates for the lane's HEAD, one a
//! template, ordered by urgency then arrival — never by feerate (MEDIUM 4). They are never in the H-1
//! lane's own order, so the H-1 walk is P2-9's exactly and M1 changes the lane only by its head; and
//! a LAPSING row's representative holds a place in the reserve, within a quarter of it
//! ([`PALW_READINESS_RESERVE_SHARE_DIVISOR`], MEDIUM 5). Every other proof is an ordinary
//! transaction.
//!
//! **Arrival, not mass, breaks a feerate tie** (review finding 5): lighter-first let five 8-byte
//! signatures out-rank one honest ML-DSA-87 filing at the same feerate, and a filer cannot choose
//! to be earlier than the object it answers. The index is kept sorted at insertion, so a template
//! build walks it instead of sorting it (review finding 2).

use kaspa_consensus_core::palw_heartbeat_carriers_v1::PalwH1LaneKeyV1;
use kaspa_consensus_core::palw_readiness_escalation_v1::{PalwReadinessCarrierV1, PalwReadinessUrgencyV1};
use kaspa_consensus_core::tx::TransactionId;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::mempool::config::Config;

/// **The carrier lane may take at most `1 / PALW_H1_CARRIER_LANE_MASS_DIVISOR` of a block's mass.**
///
/// The gate admits only what the tip's fold would take, but an honest-looking flood is still
/// possible (a bond's 64 reporter commitments, accusations from many bonds), so the lane is
/// bounded, and half a block is the bound: the other half stays the fee market's, which is also
/// what keeps licences, receipts and quorums (none of them H-1 kinds) from being starved by a DA or
/// disclosure storm (the Phase 2 plan's §5.7). Honest demand is far below it: one carrier in flight
/// per panel (`MAX_INFLIGHT_CARRIERS`), a few tens of thousands of mass each. A carrier heavier than
/// half a block rides the ordinary lane on its feerate.
pub(crate) const PALW_H1_CARRIER_LANE_MASS_DIVISOR: u64 = 2;

/// **The most carriers a template build looks at**, in lane order. The lane fills long before an
/// honest pool reaches it; the bound is what keeps a carrier-heavy pool from making every template
/// walk it whole.
pub(crate) const PALW_H1_CARRIER_LANE_SCAN: usize = 1_024;

/// The reserve's count ceiling: the carriers a full pool keeps room for whatever ordinary traffic
/// pays. A licence halt's honest demand is one carrier in flight per panel plus the reporters'
/// commitments — tens to hundreds — so a thousand is a margin, not a target.
pub(crate) const PALW_H1_CARRIER_RESERVE_TXS: usize = 1_024;

/// The reserve's byte ceiling: a thousand carriers of the heaviest honest kind (an ML-DSA-87
/// accusation or answer with its funding input, ~15 KB; a held accusation's binding, a few tens of
/// KB) fit with room.
pub(crate) const PALW_H1_CARRIER_RESERVE_BYTES: usize = 32 * 1024 * 1024;

/// The reserve never exceeds `1 / PALW_H1_CARRIER_RESERVE_POOL_FRACTION` of the pool's own limits,
/// so a small pool (a scaled-down node, a test fixture) still keeps most of itself for fees.
pub(crate) const PALW_H1_CARRIER_RESERVE_POOL_FRACTION: usize = 8;

/// **Possession proofs hold at most `1 / PALW_READINESS_RESERVE_SHARE_DIVISOR` of the reserve**, by
/// count and by bytes (the M1 review, MEDIUM 5). Only a lapsing row's proof holds a place at all, and
/// how many rows count at once is bounded by the chain (each needs an accepted proof within the row
/// age — tens on testnet-12), so honest demand is far below it; the share is what keeps a
/// ~50 KB-a-proof flood from ever taking the room a conviction arriving at a full pool needs (V-8).
pub(crate) const PALW_READINESS_RESERVE_SHARE_DIVISOR: usize = 4;

/// **The room a full mempool keeps for H-1's carriers** (P2-9 review, finding 1).
///
/// The lane only orders what is already in the pool, and a min-fee carrier is the first
/// transaction a full pool refuses and the first it evicts — so under congestion the accused could
/// keep every mempool full for about one block's fees a slot, and the conviction that should stop it
/// would enter none (V-8's case). So some carriers hold a RESERVED place ([`PalwCarrierIndexV1`]
/// decides which), and for those:
///
/// * an incoming carrier that will be reserved evicts the cheapest ordinary transactions WHATEVER
///   THEY PAY (never a reserved carrier, never its own ancestor), and
/// * no incoming transaction, whatever it pays, evicts a reserved carrier or a transaction a
///   reserved carrier spends from.
///
/// A carrier is reserved while the reserve has room by count and bytes AND no reserved carrier
/// already holds its lane key ([`PalwH1LaneKeyV1`]). The key is what keeps the reserve from being
/// one party's: the gate judges each carrier alone against the tip, so one bond's reporter could
/// file a thousand commitments that each pass it and fill a first-come reserve before the
/// conviction arrives; keyed, that bond holds one reserved place, a claim's DA session one, an
/// offence's evidence one. Every other carrier — a second of a key, one past the reserve — is an
/// ordinary transaction and competes on feerate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwCarrierReserveV1 {
    pub txs: usize,
    pub bytes: usize,
}

impl PalwCarrierReserveV1 {
    /// The reserve of a pool with `config`'s limits.
    pub(crate) fn of(config: &Config) -> Self {
        Self {
            txs: PALW_H1_CARRIER_RESERVE_TXS.min((config.maximum_transaction_count / PALW_H1_CARRIER_RESERVE_POOL_FRACTION).max(1)),
            bytes: PALW_H1_CARRIER_RESERVE_BYTES.min((config.mempool_size_limit / PALW_H1_CARRIER_RESERVE_POOL_FRACTION).max(1)),
        }
    }

    /// Whether carriers holding `txs` transactions and `bytes` bytes are inside the reserve.
    pub(crate) fn holds(&self, txs: usize, bytes: usize) -> bool {
        txs <= self.txs && bytes <= self.bytes
    }

    /// Whether possession proofs holding `txs` transactions and `bytes` bytes are inside their share.
    pub(crate) fn readiness_share_holds(&self, txs: usize, bytes: usize) -> bool {
        txs <= (self.txs / PALW_READINESS_RESERVE_SHARE_DIVISOR).max(1)
            && bytes <= (self.bytes / PALW_READINESS_RESERVE_SHARE_DIVISOR).max(1)
    }
}

/// **M1: what the tip read said of a possession proof as it entered the pool** — the proof, and how
/// urgently its row at the tip needs it (`ConsensusApi::palw_readiness_urgency_v1`; `None`: it does
/// not escalate). Asked once by the mempool's insertion path and handed to both the full pool's
/// admission and the insertion, so the proof that was let evict for the reserve is the proof the
/// reserve then holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwReadinessAdmissionV1 {
    pub carrier: PalwReadinessCarrierV1,
    pub urgency: Option<PalwReadinessUrgencyV1>,
}

/// A carrier's place in the lane: feerate descending (compared by cross-multiplication, so exactly
/// and identically on every node), then arrival, then id. `seq` is unique per insertion, so two
/// keys compare `Equal` only when they are the same entry — the order is total and agrees with `Eq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwCarrierOrderKeyV1 {
    pub fee: u64,
    pub mass: u64,
    pub seq: u64,
    pub id: TransactionId,
}

impl Ord for PalwCarrierOrderKeyV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.fee as u128 * self.mass as u128)
            .cmp(&(self.fee as u128 * other.mass as u128))
            .then(self.seq.cmp(&other.seq))
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl PartialOrd for PalwCarrierOrderKeyV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// **M1: a representative's place in the head's order** — urgency (the row closest to lapsing
/// first, a lapsed row after every lapsing one), then arrival, then id. Feerate plays no part: honest
/// proofs pay the relay minimum on compute mass, so a feerate order ranked the class with the lighter
/// proofs first every time and lapsed the heavier one (the M1 review, MEDIUM 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PalwReadinessHeadKeyV1 {
    urgency: PalwReadinessUrgencyV1,
    seq: u64,
    id: TransactionId,
}

/// **M1: what the index knows of a possession proof.**
#[derive(Clone, Debug)]
struct PalwReadinessEntryV1 {
    carrier: PalwReadinessCarrierV1,
    /// The tip read's answer: `None` while the proof does not escalate.
    urgency: Option<PalwReadinessUrgencyV1>,
    /// No parent in the pool: a chained proof cannot be mined before its parent, so it cannot lead.
    ready: bool,
    /// Its mass fits the lane's budget: a heavier proof can never lead and rides the fee market.
    fits: bool,
    /// Its key in the head order while it is its row's representative.
    head: Option<PalwReadinessHeadKeyV1>,
}

impl PalwReadinessEntryV1 {
    /// May this copy represent its row: it escalates, it is ready, and it can lead.
    fn eligible(&self) -> bool {
        self.urgency.is_some() && self.ready && self.fits
    }
}

#[derive(Clone, Debug)]
struct PalwCarrierEntryV1 {
    order: PalwCarrierOrderKeyV1,
    lane_key: Option<PalwH1LaneKeyV1>,
    bytes: usize,
    /// Whether this carrier holds a place in the reserve ([`PalwCarrierReserveV1`]).
    reserved: bool,
    /// **M1: the possession proof this entry is**; `None` for an H-1 carrier.
    readiness: Option<PalwReadinessEntryV1>,
}

/// **The carriers in the pool, kept in lane order, and which of them the reserve holds.** Written
/// only at the pool's one insertion site and its one removal site (and, for a proof, when its tip
/// read or its readiness moves), so it cannot drift from `all_transactions`; a template build reads
/// it in order and decodes nothing.
///
/// Reservation is decided at insertion — whether the pool is full or not, since a carrier that
/// entered an idle pool is exactly the one better-paying traffic would evict first once it fills —
/// and a place a reserved carrier leaves goes to the best unreserved carrier that fits
/// ([`Self::remove`]): an H-1 carrier within [`PALW_H1_CARRIER_LANE_SCAN`] of the lane order first,
/// then the most urgent lapsing row's proof. A row whose representative changes passes its place to
/// the new one.
pub(crate) struct PalwCarrierIndexV1 {
    /// The H-1 carriers in lane order — P2-9's walk, which no possession proof enters.
    order: BTreeSet<PalwCarrierOrderKeyV1>,
    /// **M1: the rows' representatives in head order** (urgency, then arrival): one per row.
    heads: BTreeSet<PalwReadinessHeadKeyV1>,
    /// **M1: each row's eligible copies** (escalating, ready, within the lane), best first: the first
    /// is the row's representative.
    copies: HashMap<PalwH1LaneKeyV1, BTreeSet<PalwCarrierOrderKeyV1>>,
    /// **M1: each row's representative.**
    representative: HashMap<PalwH1LaneKeyV1, TransactionId>,
    entries: HashMap<TransactionId, PalwCarrierEntryV1>,
    bytes: usize,
    next_seq: u64,
    reserve: PalwCarrierReserveV1,
    /// The lane's mass budget: a proof heavier than it can never lead.
    lane_budget: u64,
    reserved_txs: usize,
    reserved_bytes: usize,
    reserved_keys: HashSet<PalwH1LaneKeyV1>,
    /// The places possession proofs hold, inside [`PalwCarrierReserveV1::readiness_share_holds`].
    reserved_readiness_txs: usize,
    reserved_readiness_bytes: usize,
}

impl PalwCarrierIndexV1 {
    pub(crate) fn new(reserve: PalwCarrierReserveV1, lane_budget: u64) -> Self {
        Self {
            order: Default::default(),
            heads: Default::default(),
            copies: Default::default(),
            representative: Default::default(),
            entries: Default::default(),
            bytes: 0,
            next_seq: 0,
            reserve,
            lane_budget,
            reserved_txs: 0,
            reserved_bytes: 0,
            reserved_keys: Default::default(),
            reserved_readiness_txs: 0,
            reserved_readiness_bytes: 0,
        }
    }

    /// **Would a carrier of `bytes` under `lane_key` be reserved if it entered now?** The one rule
    /// both the insertion and the full pool's admission (`limit_transaction_count`) read, so the
    /// carrier that was let evict for the reserve is the carrier the reserve then holds.
    pub(crate) fn would_reserve(&self, bytes: usize, lane_key: Option<&PalwH1LaneKeyV1>) -> bool {
        self.reserve.holds(self.reserved_txs + 1, self.reserved_bytes + bytes)
            && lane_key.is_none_or(|key| !self.reserved_keys.contains(key))
    }

    /// [`Self::would_reserve`] for a possession proof: also inside the proofs' share of the reserve.
    fn would_reserve_readiness(&self, bytes: usize, key: &PalwH1LaneKeyV1) -> bool {
        self.would_reserve(bytes, Some(key))
            && self.reserve.readiness_share_holds(self.reserved_readiness_txs + 1, self.reserved_readiness_bytes + bytes)
    }

    /// **M1: would a possession proof entering now take a reserved place?** Its row is lapsing, it is
    /// ready and within the lane, and the reserve and the proofs' share have room for its row's key —
    /// a row that already holds a place (its representative's) buys no second one for a copy — which
    /// is what the insertion then does, so the proof that was let evict for the reserve is the proof
    /// the reserve holds.
    pub(crate) fn would_reserve_readiness_admission(
        &self,
        admission: &PalwReadinessAdmissionV1,
        bytes: usize,
        mass: u64,
        ready: bool,
    ) -> bool {
        let key = admission.carrier.lane_key();
        admission.urgency.is_some_and(|urgency| urgency.is_lapsing())
            && ready
            && mass <= self.lane_budget
            && self.would_reserve_readiness(bytes, &key)
    }

    /// Index a carrier that just entered the pool: its fee and mass as the frontier weighs them, its
    /// estimated bytes as the pool counts them, and its lane key; reserved if [`Self::would_reserve`].
    pub(crate) fn insert(&mut self, id: TransactionId, fee: u64, mass: u64, bytes: usize, lane_key: Option<PalwH1LaneKeyV1>) {
        // The pool never adds an id twice (it asserts so); keep the index exact regardless.
        self.remove(&id);
        let order = self.next_order(fee, mass, id);
        let reserved = self.would_reserve(bytes, lane_key.as_ref());
        if reserved {
            self.take_place(bytes, lane_key.as_ref(), false);
        }
        self.entries.insert(id, PalwCarrierEntryV1 { order, lane_key, bytes, reserved, readiness: None });
        self.order.insert(order);
        self.bytes += bytes;
    }

    /// **M1: index a possession proof that just entered the pool** — every one, so a proof that starts
    /// escalating later needs no decode: what the tip read said of it (`urgency`) and whether it is
    /// `ready`. It becomes its row's representative — the row's one carrier — if it is the best
    /// eligible copy.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_readiness(
        &mut self,
        id: TransactionId,
        fee: u64,
        mass: u64,
        bytes: usize,
        carrier: PalwReadinessCarrierV1,
        urgency: Option<PalwReadinessUrgencyV1>,
        ready: bool,
    ) {
        self.remove(&id);
        let order = self.next_order(fee, mass, id);
        let key = carrier.lane_key();
        let readiness = PalwReadinessEntryV1 { carrier, urgency, ready, fits: mass <= self.lane_budget, head: None };
        if readiness.eligible() {
            self.copies.entry(key.clone()).or_default().insert(order);
        }
        self.entries
            .insert(id, PalwCarrierEntryV1 { order, lane_key: Some(key.clone()), bytes, reserved: false, readiness: Some(readiness) });
        self.bytes += bytes;
        self.reselect(&key);
    }

    fn next_order(&mut self, fee: u64, mass: u64, id: TransactionId) -> PalwCarrierOrderKeyV1 {
        let order = PalwCarrierOrderKeyV1 { fee, mass, seq: self.next_seq, id };
        self.next_seq += 1;
        order
    }

    /// Forget a transaction that left the pool (a no-op for one that is not indexed). A reserved
    /// carrier's place passes to the best unreserved carrier that fits it; a row's representative is
    /// replaced by the row's next eligible copy.
    pub(crate) fn remove(&mut self, id: &TransactionId) {
        let Some(entry) = self.entries.remove(id) else { return };
        self.bytes -= entry.bytes;
        match &entry.readiness {
            None => {
                self.order.remove(&entry.order);
                if entry.reserved {
                    self.release_place(entry.bytes, entry.lane_key.as_ref(), false);
                    self.promote();
                }
            }
            Some(readiness) => {
                let key = entry.lane_key.clone().expect("a proof is keyed by its row");
                if let Some(copies) = self.copies.get_mut(&key) {
                    copies.remove(&entry.order);
                    if copies.is_empty() {
                        self.copies.remove(&key);
                    }
                }
                if self.representative.get(&key) == Some(id) {
                    self.representative.remove(&key);
                    if let Some(head) = readiness.head {
                        self.heads.remove(&head);
                    }
                    if entry.reserved {
                        self.release_place(entry.bytes, Some(&key), true);
                    }
                }
                self.reselect(&key);
                if entry.reserved {
                    self.promote();
                }
            }
        }
    }

    /// **M1: the tip read's new answer on a proof** (at a new block: the DAA moved, rows renewed). It
    /// joins or leaves its row's eligible copies, and the row's representative, its place in the head
    /// order and its reservation follow. A no-op for an H-1 carrier and for an id the index does not
    /// hold.
    pub(crate) fn set_readiness_urgency(&mut self, id: &TransactionId, urgency: Option<PalwReadinessUrgencyV1>) {
        self.update_readiness(id, |readiness| readiness.urgency = urgency);
    }

    /// **M1: a proof's last parent left the pool** (`TransactionsPool::remove_transaction`): it can
    /// now be mined, so it may represent its row.
    pub(crate) fn set_ready(&mut self, id: &TransactionId) {
        self.update_readiness(id, |readiness| readiness.ready = true);
    }

    fn update_readiness(&mut self, id: &TransactionId, update: impl FnOnce(&mut PalwReadinessEntryV1)) {
        let Some(entry) = self.entries.get_mut(id) else { return };
        let (order, key) = (entry.order, entry.lane_key.clone());
        let Some(readiness) = entry.readiness.as_mut() else { return };
        let key = key.expect("a proof is keyed by its row");
        let was = readiness.eligible();
        update(readiness);
        let is = readiness.eligible();
        if was && !is {
            if let Some(copies) = self.copies.get_mut(&key) {
                copies.remove(&order);
                if copies.is_empty() {
                    self.copies.remove(&key);
                }
            }
        } else if !was && is {
            self.copies.entry(key.clone()).or_default().insert(order);
        }
        self.reselect(&key);
    }

    /// **The row's representative is its best eligible copy.** When it changes, the old one leaves
    /// the head order and hands its place to the new one when the new one may hold it (the place is
    /// otherwise freed to the best carrier that fits); when it stays, its head key and its place follow
    /// its urgency — only a LAPSING row's proof holds a place.
    fn reselect(&mut self, key: &PalwH1LaneKeyV1) {
        let best = self.copies.get(key).and_then(|copies| copies.first()).map(|order| order.id);
        let current = self.representative.get(key).copied();
        let mut freed = false;
        if current != best
            && let Some(old) = current
        {
            self.representative.remove(key);
            if let Some(entry) = self.entries.get_mut(&old) {
                if let Some(head) = entry.readiness.as_mut().and_then(|readiness| readiness.head.take()) {
                    self.heads.remove(&head);
                }
                if std::mem::replace(&mut entry.reserved, false) {
                    let bytes = entry.bytes;
                    self.release_place(bytes, Some(key), true);
                    freed = true;
                }
            }
        }
        if let Some(new) = best
            && let Some(entry) = self.entries.get_mut(&new)
        {
            self.representative.insert(key.clone(), new);
            let (order, bytes, reserved) = (entry.order, entry.bytes, entry.reserved);
            let readiness = entry.readiness.as_mut().expect("a copy is a proof");
            let urgency = readiness.urgency.expect("an eligible copy escalates");
            let head = PalwReadinessHeadKeyV1 { urgency, seq: order.seq, id: new };
            if let Some(stale) = readiness.head.replace(head) {
                self.heads.remove(&stale);
            }
            self.heads.insert(head);
            if reserved && !urgency.is_lapsing() {
                self.entries.get_mut(&new).expect("held above").reserved = false;
                self.release_place(bytes, Some(key), true);
                freed = true;
            } else if !reserved && urgency.is_lapsing() && self.would_reserve_readiness(bytes, key) {
                self.entries.get_mut(&new).expect("held above").reserved = true;
                self.take_place(bytes, Some(key), true);
            }
        }
        if freed {
            self.promote();
        }
    }

    /// The possession proofs the index holds, eligible or not, in lane order (feerate, then arrival)
    /// — what the pool asks the tip about at a new block, so the proofs a nearly full reserve takes are
    /// the best-paying and earliest, never a hash map's pick.
    pub(crate) fn readiness_entries(&self) -> Vec<(TransactionId, PalwReadinessCarrierV1)> {
        let mut proofs: Vec<_> = self
            .entries
            .values()
            .filter_map(|entry| entry.readiness.as_ref().map(|readiness| (entry.order, readiness.carrier)))
            .collect();
        proofs.sort_by(|a, b| a.0.cmp(&b.0));
        proofs.into_iter().map(|(order, carrier)| (order.id, carrier)).collect()
    }

    /// A reserved place is given up (the caller then promotes, once its own bookkeeping is done).
    fn release_place(&mut self, bytes: usize, lane_key: Option<&PalwH1LaneKeyV1>, readiness: bool) {
        self.reserved_txs -= 1;
        self.reserved_bytes -= bytes;
        if let Some(key) = lane_key {
            self.reserved_keys.remove(key);
        }
        if readiness {
            self.reserved_readiness_txs -= 1;
            self.reserved_readiness_bytes -= bytes;
        }
    }

    fn take_place(&mut self, bytes: usize, lane_key: Option<&PalwH1LaneKeyV1>, readiness: bool) {
        self.reserved_txs += 1;
        self.reserved_bytes += bytes;
        if let Some(key) = lane_key {
            self.reserved_keys.insert(key.clone());
        }
        if readiness {
            self.reserved_readiness_txs += 1;
            self.reserved_readiness_bytes += bytes;
        }
    }

    /// **Free places go to the carriers that wait for one**: the H-1 carriers first, in lane order
    /// (within [`PALW_H1_CARRIER_LANE_SCAN`]), then the representatives of lapsing rows, most urgent
    /// first, inside the proofs' share.
    fn promote(&mut self) {
        let mut chosen: Vec<(TransactionId, bool)> = Vec::new();
        let (mut txs, mut bytes) = (self.reserved_txs, self.reserved_bytes);
        let (mut readiness_txs, mut readiness_bytes) = (self.reserved_readiness_txs, self.reserved_readiness_bytes);
        let mut keys_taken: HashSet<&PalwH1LaneKeyV1> = HashSet::new();
        for key in self.order.iter().take(PALW_H1_CARRIER_LANE_SCAN) {
            if txs >= self.reserve.txs {
                break;
            }
            let Some(candidate) = self.entries.get(&key.id) else { continue };
            if candidate.reserved
                || !self.reserve.holds(txs + 1, bytes + candidate.bytes)
                || candidate.lane_key.as_ref().is_some_and(|key| self.reserved_keys.contains(key) || keys_taken.contains(key))
            {
                continue;
            }
            txs += 1;
            bytes += candidate.bytes;
            keys_taken.extend(candidate.lane_key.as_ref());
            chosen.push((key.id, false));
        }
        for head in self.heads.iter().take(PALW_H1_CARRIER_LANE_SCAN) {
            if txs >= self.reserve.txs {
                break;
            }
            let Some(candidate) = self.entries.get(&head.id) else { continue };
            if candidate.reserved
                || !head.urgency.is_lapsing()
                || !self.reserve.holds(txs + 1, bytes + candidate.bytes)
                || !self.reserve.readiness_share_holds(readiness_txs + 1, readiness_bytes + candidate.bytes)
                || candidate.lane_key.as_ref().is_some_and(|key| self.reserved_keys.contains(key))
            {
                continue;
            }
            txs += 1;
            bytes += candidate.bytes;
            readiness_txs += 1;
            readiness_bytes += candidate.bytes;
            chosen.push((head.id, true));
        }
        for (id, readiness) in chosen {
            let entry = self.entries.get_mut(&id).expect("chosen above");
            entry.reserved = true;
            let (bytes, key) = (entry.bytes, entry.lane_key.clone());
            self.take_place(bytes, key.as_ref(), readiness);
        }
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: &TransactionId) -> bool {
        self.entries.contains_key(id)
    }

    /// Whether `id` is a carrier the reserve holds — one no admission may evict.
    pub(crate) fn is_reserved(&self, id: &TransactionId) -> bool {
        self.entries.get(id).is_some_and(|entry| entry.reserved)
    }

    /// **M1: whether `id` is its row's representative** — the row's one carrier.
    #[cfg(test)]
    pub(crate) fn is_representative(&self, id: &TransactionId) -> bool {
        self.entries.get(id).and_then(|entry| entry.lane_key.as_ref()).is_some_and(|key| self.representative.get(key) == Some(id))
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The estimated bytes the indexed carriers occupy.
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// How many carriers the reserve holds, and their bytes.
    #[cfg(test)]
    pub(crate) fn reserved(&self) -> (usize, usize) {
        (self.reserved_txs, self.reserved_bytes)
    }

    /// How many possession proofs the reserve holds, and their bytes.
    #[cfg(test)]
    pub(crate) fn reserved_readiness(&self) -> (usize, usize) {
        (self.reserved_readiness_txs, self.reserved_readiness_bytes)
    }

    /// The H-1 carriers in lane order: `(id, mass, lane key)`. No possession proof is here: M1's
    /// proofs reach the lane only as its head ([`Self::readiness_heads`]).
    pub(crate) fn in_lane_order(&self) -> impl Iterator<Item = (TransactionId, u64, Option<&PalwH1LaneKeyV1>)> + '_ {
        self.order.iter().map(move |key| (key.id, key.mass, self.entries.get(&key.id).and_then(|entry| entry.lane_key.as_ref())))
    }

    /// **M1: the rows' representatives in head order** (urgency, then arrival): `(id, mass)` — the
    /// candidates for the head of the lane (`TransactionsPool::build_palw_carrier_lane`). One per row,
    /// each ready and within the lane.
    pub(crate) fn readiness_heads(&self) -> impl Iterator<Item = (TransactionId, u64)> + '_ {
        self.heads.iter().filter_map(move |head| self.entries.get(&head.id).map(|entry| (head.id, entry.order.mass)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash64;

    fn id(n: u64) -> TransactionId {
        TransactionId::from_u64_word(n)
    }

    /// The lane budget the index tests use: a proof heavier than this can never lead.
    const LANE: u64 = 5_000;

    fn roomy() -> PalwCarrierIndexV1 {
        PalwCarrierIndexV1::new(PalwCarrierReserveV1 { txs: PALW_H1_CARRIER_RESERVE_TXS, bytes: PALW_H1_CARRIER_RESERVE_BYTES }, LANE)
    }

    /// **Feerate first, then arrival**: a later carrier paying more goes first; at an equal feerate
    /// the earlier one does, whatever either weighs (review finding 5's probe B: five light junk
    /// accusations filed after one heavy honest one no longer out-rank it).
    #[test]
    fn the_lane_order_is_feerate_then_arrival() {
        let mut index = roomy();
        index.insert(id(1), 4_000, 4_000, 10, None); // honest: feerate 1, heavy, first
        for n in 2..7 {
            index.insert(id(n), 1_000, 1_000, 10, None); // junk: feerate 1, light, later
        }
        index.insert(id(7), 20_000, 2_000, 10, None); // feerate 10, last
        let order: Vec<_> = index.in_lane_order().map(|(id, ..)| id).collect();
        assert_eq!(order, vec![id(7), id(1), id(2), id(3), id(4), id(5), id(6)]);
    }

    /// The index follows the pool exactly: bytes and count come back to zero, and a lane key rides
    /// with its carrier.
    #[test]
    fn the_index_counts_what_it_holds() {
        let mut index = roomy();
        let key = PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(9));
        index.insert(id(1), 1_000, 1_000, 300, Some(key.clone()));
        index.insert(id(2), 1_000, 1_000, 200, None);
        assert_eq!((index.len(), index.bytes()), (2, 500));
        assert_eq!(index.in_lane_order().next().map(|(id, mass, key)| (id, mass, key.cloned())), Some((id(1), 1_000, Some(key))));
        index.remove(&id(1));
        index.remove(&id(3));
        assert_eq!((index.len(), index.bytes(), index.contains(&id(1))), (1, 200, false));
        index.remove(&id(2));
        assert!(index.is_empty() && index.bytes() == 0 && index.reserved() == (0, 0));
    }

    /// **One reserved place per lane key, and a place that frees goes to the best carrier that fits
    /// it** (the reserve's own flood: one bond's reporter filing many commitments that each pass the
    /// gate must not fill a first-come reserve ahead of the conviction).
    #[test]
    fn the_reserve_holds_one_carrier_per_key_and_passes_freed_places_on() {
        let reporter = PalwH1LaneKeyV1::Reporter(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(
            kaspa_consensus_core::tx::TransactionOutpoint::new(id(0xB0), 0),
        ));
        let session = |claim: u64| Some(PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(claim)));
        let mut index = PalwCarrierIndexV1::new(PalwCarrierReserveV1 { txs: 3, bytes: 1_000 }, LANE);

        // A reporter's flood: the first commitment holds the bond's place, the rest are ordinary.
        for n in 1..=10 {
            assert_eq!(index.would_reserve(100, Some(&reporter)), n == 1);
            index.insert(id(n), 1_000, 1_000, 100, Some(reporter.clone()));
        }
        assert_eq!(index.reserved(), (1, 100));
        assert!(index.is_reserved(&id(1)) && !index.is_reserved(&id(2)));

        // The conviction still finds a place, and so does an unkeyed court move.
        assert!(index.would_reserve(300, session(7).as_ref()));
        index.insert(id(20), 1_000, 1_000, 300, session(7));
        index.insert(id(21), 1_000, 1_000, 300, None);
        assert!(index.is_reserved(&id(20)) && index.is_reserved(&id(21)));
        assert_eq!(index.reserved(), (3, 700));
        // Full by count: nothing else is reserved, whatever its key.
        assert!(!index.would_reserve(1, session(8).as_ref()));
        index.insert(id(22), 5_000, 1_000, 200, session(8));
        assert!(!index.is_reserved(&id(22)));

        // The reporter's reserved commitment is mined: its place goes to the best unreserved
        // carrier that fits — the richer accusation of claim 8, not the bond's next commitment.
        index.remove(&id(1));
        assert!(index.is_reserved(&id(22)), "the freed place passes on in lane order");
        assert!((2..=10).all(|n| !index.is_reserved(&id(n))), "and the key rule still applies: {:?}", index.reserved());
        assert_eq!(index.reserved(), (3, 800));

        // Removing an unreserved carrier changes no place.
        index.remove(&id(5));
        assert_eq!(index.reserved(), (3, 800));
        // The conviction is mined: its place goes to the bond's next commitment, now that the bond
        // holds none — the earliest of the feerate tie.
        index.remove(&id(20));
        assert!(index.is_reserved(&id(2)) && (3..=10).all(|n| !index.is_reserved(&id(n))));
    }

    fn proof(bond: u64, class: u64) -> PalwReadinessCarrierV1 {
        PalwReadinessCarrierV1 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(id(bond), 0)),
            class_id: Hash64::from_u64_word(class),
            span: 1,
            proof_version: 2,
        }
    }

    const LAPSING: Option<PalwReadinessUrgencyV1> = Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 108 });
    const LAPSED: Option<PalwReadinessUrgencyV1> = Some(PalwReadinessUrgencyV1::Lapsed);

    fn heads(index: &PalwCarrierIndexV1) -> Vec<TransactionId> {
        index.readiness_heads().map(|(id, _)| id).collect()
    }

    /// **M1 (the M1 review, HIGH 1): one carrier per row, however many copies a bond files.** 1,025
    /// escalating copies of one `(bond, class)` row paying far more than an honest proof: one of them
    /// represents the row — one head candidate, one reserved place — and the rest are ordinary
    /// transactions. The honest row keeps its own head candidate behind them, and no copy is in the
    /// H-1 lane's order at all. When the representative leaves, the row's next copy takes over, place
    /// and all.
    #[test]
    fn a_row_is_one_carrier_however_many_copies_it_has() {
        let mut index = roomy();
        index.insert(id(1), 1_000, 1_000, 100, Some(PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(1)))); // a conviction
        for n in 0..(PALW_H1_CARRIER_LANE_SCAN as u64 + 1) {
            index.insert_readiness(id(10_000 + n), 1_000_000, 2_000, 100, proof(66, 7), LAPSING, true);
        }
        index.insert_readiness(
            id(2),
            2_000,
            2_000,
            100,
            proof(1, 7),
            Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 108 }),
            true,
        );
        assert_eq!(heads(&index), vec![id(10_000), id(2)], "one head candidate per row: the flooder's best copy, then the honest row");
        assert_eq!(index.in_lane_order().map(|(id, ..)| id).collect::<Vec<_>>(), vec![id(1)], "the H-1 order is P2-9's alone");
        assert_eq!(index.reserved_readiness(), (2, 200), "one place per row");
        assert!(index.is_representative(&id(10_000)) && !index.is_representative(&id(10_001)));
        index.remove(&id(10_000));
        assert_eq!(heads(&index), vec![id(10_001), id(2)], "the row's next copy represents it");
        assert!(index.is_reserved(&id(10_001)) && index.reserved_readiness() == (2, 200), "and takes its place");
    }

    /// **M1 (the M1 review, HIGH 1): a copy that cannot lead is no carrier** — one heavier than the
    /// lane (it could never be its head) and one chained on a parent still in the pool (it cannot be
    /// mined first) neither represent their row nor take a place, so a flood of either, which is never
    /// mined and costs its filer nothing, reaches neither the head nor the reserve. A chained copy
    /// whose parent leaves may then represent its row.
    #[test]
    fn a_copy_over_the_lane_or_behind_a_parent_is_no_carrier() {
        let mut index = roomy();
        for n in 0..(PALW_H1_CARRIER_LANE_SCAN as u64 + 1) {
            index.insert_readiness(id(10_000 + n), 1_000_000, LANE + 1_000, 100, proof(66, 7), LAPSING, true); // over the lane
            index.insert_readiness(id(20_000 + n), 1_000_000, 2_000, 100, proof(67, 7), LAPSING, false); // chained
        }
        index.insert_readiness(id(2), 2_000, 2_000, 100, proof(1, 7), LAPSING, true);
        assert_eq!(heads(&index), vec![id(2)], "only the honest proof can lead");
        assert_eq!(index.reserved_readiness(), (1, 100));
        index.set_ready(&id(20_000));
        assert!(index.is_representative(&id(20_000)), "its parent was mined: now it can lead");
        assert_eq!(heads(&index), vec![id(20_000), id(2)], "at one urgency the earlier arrival leads");
    }

    /// **M1 (the M1 review, MEDIUM 4): the head is the row closest to lapsing, then the earlier
    /// arrival — never the better payer.** A heavy proof (lower feerate, as an honest proof of a
    /// heavier class pays) whose row lapses at 108 leads a light one lapsing at 110 that arrived first
    /// and pays five times the feerate; a lapsed row's proof comes after both; at one urgency the
    /// earlier arrival leads.
    #[test]
    fn the_head_is_the_row_closest_to_lapsing_not_the_best_payer() {
        let mut index = roomy();
        let soon = Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 108 });
        let later = Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 110 });
        index.insert_readiness(id(1), 5_000, 1_000, 100, proof(1, 1), later, true); // light, rich, first
        index.insert_readiness(id(2), 1_000, 4_000, 100, proof(2, 2), LAPSED, true);
        index.insert_readiness(id(3), 1_200, 4_000, 100, proof(3, 2), soon, true); // heavy, poorer, later
        index.insert_readiness(id(4), 9_000, 1_000, 100, proof(4, 1), soon, true); // same urgency, arrived after
        assert_eq!(heads(&index), vec![id(3), id(4), id(1), id(2)]);
        // The tip read moves: row 1 now lapses sooner than anything else.
        index.set_readiness_urgency(&id(1), Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 107 }));
        assert_eq!(heads(&index), vec![id(1), id(3), id(4), id(2)]);
        // Renewed: no longer a candidate.
        index.set_readiness_urgency(&id(3), None);
        assert_eq!(heads(&index), vec![id(1), id(4), id(2)]);
    }

    /// **M1 (the M1 review, MEDIUM 5): only a lapsing row's proof holds a place, and proofs hold at
    /// most a quarter of the reserve.** With a reserve of eight, two lapsing rows hold places and a
    /// third waits; a lapsed row holds none; H-1 carriers still take every other place. When a
    /// held row lapses (its proof now recovers it rather than keeping it), its place passes to the
    /// waiting lapsing row.
    #[test]
    fn only_lapsing_rows_hold_places_and_within_a_quarter_of_the_reserve() {
        let mut index = PalwCarrierIndexV1::new(PalwCarrierReserveV1 { txs: 8, bytes: 1_000_000 }, LANE);
        index.insert_readiness(id(4), 1_000, 1_000, 100, proof(4, 7), LAPSED, true);
        assert!(!index.is_reserved(&id(4)) && index.reserved() == (0, 0), "a lapsed row: no place, with the reserve empty");
        for n in 1..=3 {
            index.insert_readiness(id(n), 1_000, 1_000, 100, proof(n, 7), LAPSING, true);
        }
        assert!(index.is_reserved(&id(1)) && index.is_reserved(&id(2)), "a quarter of eight");
        assert!(!index.is_reserved(&id(3)) && !index.is_reserved(&id(4)));
        assert_eq!(heads(&index).len(), 4, "every row still has its head candidate");
        for n in 0..10 {
            index.insert(id(100 + n), 1_000, 1_000, 100, Some(PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(n))));
        }
        assert_eq!(index.reserved(), (8, 800), "the H-1 carriers take the rest");
        assert!((100..106).all(|n| index.is_reserved(&id(n))) && !index.is_reserved(&id(106)));
        index.set_readiness_urgency(&id(1), LAPSED);
        assert!(!index.is_reserved(&id(1)), "a lapsed row holds no place");
        assert!(index.is_reserved(&id(106)), "the freed place goes to the H-1 carrier waiting first");
        index.remove(&id(100));
        assert!(index.is_reserved(&id(107)) && !index.is_reserved(&id(3)), "H-1 first");
        for n in 101..110 {
            index.remove(&id(n));
        }
        assert!(index.is_reserved(&id(3)), "then the waiting lapsing row: {:?}", index.reserved_readiness());
        assert_eq!(index.reserved_readiness(), (2, 200));
    }

    /// **M1: a proof whose row does not escalate is not a carrier at all** — not in the H-1 order, not
    /// a head candidate, no place — however many a pool holds and whatever they pay; the tip read
    /// promotes one when its row nears staleness and demotes it when the row is renewed.
    #[test]
    fn a_proof_that_does_not_escalate_is_an_ordinary_transaction() {
        let mut index = roomy();
        let flood = PALW_H1_CARRIER_LANE_SCAN as u64 + 100;
        for n in 0..flood {
            index.insert_readiness(id(10_000 + n), 1_000_000, 1_000, 100, proof(n, 7), None, true);
        }
        index.insert(id(1), 1_000, 1_000, 100, None);
        assert_eq!(index.in_lane_order().map(|(id, ..)| id).collect::<Vec<_>>(), vec![id(1)], "the flood is no carrier");
        assert!(heads(&index).is_empty());
        assert_eq!(index.readiness_entries().len(), flood as usize, "every proof is still indexed, for the next block's read");
        index.set_readiness_urgency(&id(10_000), LAPSING);
        assert_eq!(heads(&index), vec![id(10_000)]);
        assert!(index.is_reserved(&id(10_000)));
        assert_eq!(index.in_lane_order().map(|(id, ..)| id).collect::<Vec<_>>(), vec![id(1)], "never in the H-1 order");
        index.set_readiness_urgency(&id(10_000), None);
        assert!(heads(&index).is_empty() && !index.is_reserved(&id(10_000)) && index.reserved() == (1, 100));
        index.remove(&id(10_001));
        assert_eq!(index.readiness_entries().len(), flood as usize - 1);
    }

    /// The reserve is the constant, or an eighth of a smaller pool — never all of it.
    #[test]
    fn the_reserve_is_bounded_by_the_pool() {
        let full = Config::build_default(1_000, false, 500_000);
        assert_eq!(
            PalwCarrierReserveV1::of(&full),
            PalwCarrierReserveV1 { txs: PALW_H1_CARRIER_RESERVE_TXS, bytes: PALW_H1_CARRIER_RESERVE_BYTES }
        );
        let mut small = full.clone();
        small.maximum_transaction_count = 20;
        small.mempool_size_limit = 80_000;
        assert_eq!(PalwCarrierReserveV1::of(&small), PalwCarrierReserveV1 { txs: 2, bytes: 10_000 });
    }
}
