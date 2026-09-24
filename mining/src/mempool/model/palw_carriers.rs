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
//! * **The reserve** ([`PalwCarrierReserveV1`]): a carrier may take a full pool's room from the
//!   cheapest ordinary transactions whatever they pay, and no ordinary transaction may take a
//!   carrier's room, while the carriers stay inside the reserve.
//! * **The gate** (consensus: `palw_mempool_h1_carrier_refusal`): only a carrier the tip's fold would
//!   take enters the pool at all, and one it stops taking is evicted at the next template. That is
//!   what makes the other two safe to give: the lane and the reserve are sold to objects the fold
//!   folds, not to anything that decodes (P2-9 review, finding 5).
//!
//! **Arrival, not mass, breaks a feerate tie** (review finding 5): lighter-first let five 8-byte
//! signatures out-rank one honest ML-DSA-87 filing at the same feerate, and a filer cannot choose
//! to be earlier than the object it answers. The index is kept sorted at insertion, so a template
//! build walks it instead of sorting it (review finding 2).

use kaspa_consensus_core::palw_heartbeat_carriers_v1::PalwH1LaneKeyV1;
use kaspa_consensus_core::tx::TransactionId;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

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
/// would enter none (V-8's case). Inside the reserve, therefore:
///
/// * an incoming carrier evicts the cheapest ordinary transactions WHATEVER THEY PAY (never another
///   carrier, never its own ancestor), and
/// * an incoming ordinary transaction never evicts a carrier (nor a transaction a carrier spends
///   from), however much it pays.
///
/// Outside the reserve a carrier is an ordinary transaction and competes on feerate, so carriers can
/// hold at most the reserve against the fee market — and every one of them is an object the tip's
/// fold would take (the gate), which the lane then drains at up to half a block a template.
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
}

/// **The carriers in the pool, kept in lane order.** Written only at the pool's one insertion site
/// and its one removal site, so it cannot drift from `all_transactions`; a template build reads it
/// in order and decodes nothing.
#[derive(Default)]
pub(crate) struct PalwCarrierIndexV1 {
    order: BTreeSet<PalwCarrierOrderKeyV1>,
    entries: HashMap<TransactionId, PalwCarrierEntryV1>,
    bytes: usize,
    next_seq: u64,
}

impl PalwCarrierIndexV1 {
    /// Index a carrier that just entered the pool: its fee and mass as the frontier weighs them, its
    /// estimated bytes as the pool counts them, and its lane key.
    pub(crate) fn insert(&mut self, id: TransactionId, fee: u64, mass: u64, bytes: usize, lane_key: Option<PalwH1LaneKeyV1>) {
        let order = PalwCarrierOrderKeyV1 { fee, mass, seq: self.next_seq, id };
        self.next_seq += 1;
        if let Some(stale) = self.entries.insert(id, PalwCarrierEntryV1 { order, lane_key, bytes }) {
            // The pool never adds an id twice (it asserts so); keep the index exact regardless.
            self.order.remove(&stale.order);
            self.bytes -= stale.bytes;
        }
        self.order.insert(order);
        self.bytes += bytes;
    }

    /// Forget a transaction that left the pool (a no-op for one that is not a carrier).
    pub(crate) fn remove(&mut self, id: &TransactionId) {
        if let Some(entry) = self.entries.remove(id) {
            self.order.remove(&entry.order);
            self.bytes -= entry.bytes;
        }
    }

    pub(crate) fn contains(&self, id: &TransactionId) -> bool {
        self.entries.contains_key(id)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The estimated bytes the indexed carriers occupy.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// The carriers in lane order: `(id, mass, lane key)`.
    pub(crate) fn in_lane_order(&self) -> impl Iterator<Item = (TransactionId, u64, Option<&PalwH1LaneKeyV1>)> + '_ {
        self.order.iter().map(|key| (key.id, key.mass, self.entries.get(&key.id).and_then(|entry| entry.lane_key.as_ref())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash64;

    fn id(n: u64) -> TransactionId {
        TransactionId::from_u64_word(n)
    }

    /// **Feerate first, then arrival**: a later carrier paying more goes first; at an equal feerate
    /// the earlier one does, whatever either weighs (review finding 5's probe B: five light junk
    /// accusations filed after one heavy honest one no longer out-rank it).
    #[test]
    fn the_lane_order_is_feerate_then_arrival() {
        let mut index = PalwCarrierIndexV1::default();
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
        let mut index = PalwCarrierIndexV1::default();
        let key = PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(9));
        index.insert(id(1), 1_000, 1_000, 300, Some(key.clone()));
        index.insert(id(2), 1_000, 1_000, 200, None);
        assert_eq!((index.len(), index.bytes()), (2, 500));
        assert_eq!(index.in_lane_order().next().map(|(id, mass, key)| (id, mass, key.cloned())), Some((id(1), 1_000, Some(key))));
        index.remove(&id(1));
        index.remove(&id(3));
        assert_eq!((index.len(), index.bytes(), index.contains(&id(1))), (1, 200, false));
        index.remove(&id(2));
        assert!(index.is_empty() && index.bytes() == 0);
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
