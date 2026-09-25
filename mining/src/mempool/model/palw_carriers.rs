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
//! **A possession proof whose row is about to lapse is a carrier too** (the 2026-09-25
//! model-registry review, M1; `palw_readiness_escalation_v1`): past R-core+ the gate puts every proof
//! to the fold, the index keeps every proof, and a proof is ACTIVE — a carrier, keyed by its
//! `(bond, class)` row — exactly while the tip read says its row escalates
//! (`ConsensusApi::palw_readiness_escalated_v1`, asked when it enters and at every new block). An
//! inactive proof is an ordinary transaction: no place in the reserve, no slot in the lane. The lane
//! puts the first active proof at its head, one a template, so a DA storm filling the lane cannot keep
//! an honest seat's rows past staleness, and the storm keeps the rest of the lane.
//!
//! **Arrival, not mass, breaks a feerate tie** (review finding 5): lighter-first let five 8-byte
//! signatures out-rank one honest ML-DSA-87 filing at the same feerate, and a filer cannot choose
//! to be earlier than the object it answers. The index is kept sorted at insertion, so a template
//! build walks it instead of sorting it (review finding 2).

use kaspa_consensus_core::palw_heartbeat_carriers_v1::PalwH1LaneKeyV1;
use kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1;
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
}

/// **M1: what the tip read said of a possession proof as it entered the pool** — the proof, and
/// whether its row at the tip escalates (`ConsensusApi::palw_readiness_escalated_v1`). Asked once by
/// the mempool's insertion path and handed to both the full pool's admission and the insertion, so the
/// proof that was let evict for the reserve is the proof the reserve then holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwReadinessAdmissionV1 {
    pub carrier: PalwReadinessCarrierV1,
    pub escalated: bool,
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

#[derive(Clone, Debug)]
struct PalwCarrierEntryV1 {
    order: PalwCarrierOrderKeyV1,
    lane_key: Option<PalwH1LaneKeyV1>,
    bytes: usize,
    /// Whether this carrier holds a place in the reserve ([`PalwCarrierReserveV1`]).
    reserved: bool,
    /// **M1: the possession proof this entry is**, as the tip read names it; `None` for an H-1 carrier.
    readiness: Option<PalwReadinessCarrierV1>,
    /// **Whether the entry is a carrier now**: always for an H-1 carrier; for a proof, while its row
    /// at the tip escalates ([`PalwCarrierIndexV1::set_readiness_escalated`]). An inactive proof is an
    /// ordinary transaction — it holds no place and takes no lane slot.
    active: bool,
}

/// **The carriers in the pool, kept in lane order, and which of them the reserve holds.** Written
/// only at the pool's one insertion site and its one removal site, so it cannot drift from
/// `all_transactions`; a template build reads it in order and decodes nothing.
///
/// Reservation is decided at insertion — whether the pool is full or not, since a carrier that
/// entered an idle pool is exactly the one better-paying traffic would evict first once it fills —
/// and a place a reserved carrier leaves goes to the best unreserved carrier that fits
/// ([`Self::remove`]), within [`PALW_H1_CARRIER_LANE_SCAN`] of the lane order. A possession proof
/// that starts or stops escalating takes or gives up its place the same way
/// ([`Self::set_readiness_escalated`]).
pub(crate) struct PalwCarrierIndexV1 {
    order: BTreeSet<PalwCarrierOrderKeyV1>,
    entries: HashMap<TransactionId, PalwCarrierEntryV1>,
    bytes: usize,
    next_seq: u64,
    reserve: PalwCarrierReserveV1,
    reserved_txs: usize,
    reserved_bytes: usize,
    reserved_keys: HashSet<PalwH1LaneKeyV1>,
}

impl PalwCarrierIndexV1 {
    pub(crate) fn new(reserve: PalwCarrierReserveV1) -> Self {
        Self {
            order: Default::default(),
            entries: Default::default(),
            bytes: 0,
            next_seq: 0,
            reserve,
            reserved_txs: 0,
            reserved_bytes: 0,
            reserved_keys: Default::default(),
        }
    }

    /// **Would a carrier of `bytes` under `lane_key` be reserved if it entered now?** The one rule
    /// both the insertion and the full pool's admission (`limit_transaction_count`) read, so the
    /// carrier that was let evict for the reserve is the carrier the reserve then holds.
    pub(crate) fn would_reserve(&self, bytes: usize, lane_key: Option<&PalwH1LaneKeyV1>) -> bool {
        self.reserve.holds(self.reserved_txs + 1, self.reserved_bytes + bytes)
            && lane_key.is_none_or(|key| !self.reserved_keys.contains(key))
    }

    /// Index a carrier that just entered the pool: its fee and mass as the frontier weighs them, its
    /// estimated bytes as the pool counts them, and its lane key; reserved if [`Self::would_reserve`].
    pub(crate) fn insert(&mut self, id: TransactionId, fee: u64, mass: u64, bytes: usize, lane_key: Option<PalwH1LaneKeyV1>) {
        self.insert_entry(id, fee, mass, bytes, lane_key, None, true);
    }

    /// **M1: index a possession proof that just entered the pool** — every one, so a proof that starts
    /// escalating later needs no decode. It is a carrier (reserved if [`Self::would_reserve`], in the
    /// lane) only while `escalated`.
    pub(crate) fn insert_readiness(
        &mut self,
        id: TransactionId,
        fee: u64,
        mass: u64,
        bytes: usize,
        carrier: PalwReadinessCarrierV1,
        escalated: bool,
    ) {
        self.insert_entry(id, fee, mass, bytes, Some(carrier.lane_key()), Some(carrier), escalated);
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_entry(
        &mut self,
        id: TransactionId,
        fee: u64,
        mass: u64,
        bytes: usize,
        lane_key: Option<PalwH1LaneKeyV1>,
        readiness: Option<PalwReadinessCarrierV1>,
        active: bool,
    ) {
        // The pool never adds an id twice (it asserts so); keep the index exact regardless.
        self.remove(&id);
        let order = PalwCarrierOrderKeyV1 { fee, mass, seq: self.next_seq, id };
        self.next_seq += 1;
        let reserved = active && self.would_reserve(bytes, lane_key.as_ref());
        if reserved {
            self.take_place(bytes, lane_key.as_ref());
        }
        self.entries.insert(id, PalwCarrierEntryV1 { order, lane_key, bytes, reserved, readiness, active });
        self.order.insert(order);
        self.bytes += bytes;
    }

    /// Forget a transaction that left the pool (a no-op for one that is not a carrier). A reserved
    /// carrier's place passes to the best unreserved carrier that fits it, in lane order.
    pub(crate) fn remove(&mut self, id: &TransactionId) {
        let Some(entry) = self.entries.remove(id) else { return };
        self.order.remove(&entry.order);
        self.bytes -= entry.bytes;
        if entry.reserved {
            self.give_up_place(entry.bytes, entry.lane_key.as_ref());
        }
    }

    /// **M1: a proof's row starts or stops escalating at the tip.** Starting, it takes a place if
    /// [`Self::would_reserve`] (the pool is not asked to evict anything: it is already in); stopping —
    /// its row renewed, by it or by a copy — it gives its place up to the best carrier that fits.
    /// A no-op for an H-1 carrier and for an id the index does not hold.
    pub(crate) fn set_readiness_escalated(&mut self, id: &TransactionId, escalated: bool) {
        let Some(entry) = self.entries.get(id) else { return };
        if entry.readiness.is_none() || entry.active == escalated {
            return;
        }
        let (bytes, lane_key) = (entry.bytes, entry.lane_key.clone());
        if escalated {
            let reserved = self.would_reserve(bytes, lane_key.as_ref());
            if reserved {
                self.take_place(bytes, lane_key.as_ref());
            }
            let entry = self.entries.get_mut(id).expect("held above");
            entry.active = true;
            entry.reserved = reserved;
        } else {
            let entry = self.entries.get_mut(id).expect("held above");
            entry.active = false;
            if std::mem::replace(&mut entry.reserved, false) {
                self.give_up_place(bytes, lane_key.as_ref());
            }
        }
    }

    /// The possession proofs the index holds, active or not — what the pool asks the tip about at a
    /// new block.
    pub(crate) fn readiness_entries(&self) -> Vec<(TransactionId, PalwReadinessCarrierV1)> {
        self.entries.iter().filter_map(|(id, entry)| entry.readiness.map(|carrier| (*id, carrier))).collect()
    }

    /// A reserved place is given up: it passes to the best unreserved active carrier that fits.
    fn give_up_place(&mut self, bytes: usize, lane_key: Option<&PalwH1LaneKeyV1>) {
        self.reserved_txs -= 1;
        self.reserved_bytes -= bytes;
        if let Some(key) = lane_key {
            self.reserved_keys.remove(key);
        }
        // Promotion: disjoint fields, so the walk over `order` and the writes can share the body.
        for key in self.order.iter().take(PALW_H1_CARRIER_LANE_SCAN) {
            if self.reserved_txs >= self.reserve.txs {
                break;
            }
            let Some(candidate) = self.entries.get_mut(&key.id) else { continue };
            if candidate.reserved
                || !candidate.active
                || !self.reserve.holds(self.reserved_txs + 1, self.reserved_bytes + candidate.bytes)
                || candidate.lane_key.as_ref().is_some_and(|key| self.reserved_keys.contains(key))
            {
                continue;
            }
            candidate.reserved = true;
            self.reserved_txs += 1;
            self.reserved_bytes += candidate.bytes;
            if let Some(key) = &candidate.lane_key {
                self.reserved_keys.insert(key.clone());
            }
        }
    }

    fn take_place(&mut self, bytes: usize, lane_key: Option<&PalwH1LaneKeyV1>) {
        self.reserved_txs += 1;
        self.reserved_bytes += bytes;
        if let Some(key) = lane_key {
            self.reserved_keys.insert(key.clone());
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

    /// The carriers in lane order: `(id, mass, lane key)` — every H-1 carrier and every possession
    /// proof that escalates now; a proof that does not is an ordinary transaction and is not here.
    /// The walk looks at no more than [`PALW_H1_CARRIER_LANE_SCAN`] entries, the inactive included.
    pub(crate) fn in_lane_order(&self) -> impl Iterator<Item = (TransactionId, u64, Option<&PalwH1LaneKeyV1>)> + '_ {
        self.order.iter().take(PALW_H1_CARRIER_LANE_SCAN).filter_map(|key| {
            let entry = self.entries.get(&key.id)?;
            entry.active.then_some((key.id, key.mass, entry.lane_key.as_ref()))
        })
    }

    /// **M1: the escalating possession proofs in lane order** — the candidates for the head of the
    /// lane (`TransactionsPool::build_palw_carrier_lane`).
    pub(crate) fn escalated_readiness_in_lane_order(
        &self,
    ) -> impl Iterator<Item = (TransactionId, u64, Option<&PalwH1LaneKeyV1>)> + '_ {
        self.in_lane_order().filter(|(id, ..)| self.entries.get(id).is_some_and(|entry| entry.readiness.is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash64;

    fn id(n: u64) -> TransactionId {
        TransactionId::from_u64_word(n)
    }

    fn roomy() -> PalwCarrierIndexV1 {
        PalwCarrierIndexV1::new(PalwCarrierReserveV1 { txs: PALW_H1_CARRIER_RESERVE_TXS, bytes: PALW_H1_CARRIER_RESERVE_BYTES })
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
        let mut index = PalwCarrierIndexV1::new(PalwCarrierReserveV1 { txs: 3, bytes: 1_000 });

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
