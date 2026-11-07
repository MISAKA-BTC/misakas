//! A bounded index of carriers qualified against original paid native rows.
//! Route parsing alone cannot insert an entry. Template selection qualifies
//! every entry again against its own exact selected-parent snapshot.
use kaspa_consensus_core::{palw_native_carrier_index_v22::NativeCarrierCandidateV22, tx::TransactionId};
use std::collections::{BTreeMap, HashMap};

type UnitKey = (u8, u8, [u8; 64], u16);
type OrderKey = (u64, UnitKey, TransactionId);

pub(crate) struct NativeCarrierAdmissionV22 {
    pub candidate: NativeCarrierCandidateV22,
    pub deadline: u64,
    pub bytes: usize,
    pub pool_bytes: usize,
}

pub(crate) struct NativeCarrierIndexV22 {
    limit: usize,
    byte_limit: usize,
    bytes: usize,
    pool_bytes: usize,
    ordered: BTreeMap<OrderKey, NativeCarrierCandidateV22>,
    by_tx: HashMap<TransactionId, OrderKey>,
    by_unit: HashMap<UnitKey, usize>,
    sizes: HashMap<TransactionId, usize>,
    pool_sizes: HashMap<TransactionId, usize>,
}

impl NativeCarrierIndexV22 {
    pub fn new(limit: usize, byte_limit: usize) -> Self {
        Self {
            limit,
            byte_limit,
            bytes: 0,
            pool_bytes: 0,
            ordered: Default::default(),
            by_tx: Default::default(),
            by_unit: Default::default(),
            sizes: Default::default(),
            pool_sizes: Default::default(),
        }
    }
    pub fn can_insert(&self, id: TransactionId, a: &NativeCarrierAdmissionV22) -> bool {
        self.can_insert_replacing(id, a, std::iter::empty())
    }
    pub fn can_insert_replacing(
        &self,
        id: TransactionId,
        a: &NativeCarrierAdmissionV22,
        replaced: impl Iterator<Item = TransactionId>,
    ) -> bool {
        if a.pool_bytes > a.bytes {
            return false;
        }
        let mut removed = std::collections::HashSet::new();
        let mut bytes = self.bytes;
        let mut count = self.by_tx.len();
        let mut aliases = self.by_unit.get(&a.candidate.key()).copied().unwrap_or(0);
        for id in replaced {
            if !removed.insert(id) {
                continue;
            }
            if let Some(key) = self.by_tx.get(&id) {
                count -= 1;
                bytes -= self.sizes.get(&id).expect("indexed native size");
                if key.1 == a.candidate.key() {
                    aliases -= 1;
                }
            }
        }
        self.by_tx.contains_key(&id)
            || count < self.limit && bytes.checked_add(a.bytes).is_some_and(|n| n <= self.byte_limit) && aliases < 2
    }
    pub fn insert(&mut self, id: TransactionId, a: NativeCarrierAdmissionV22) -> bool {
        if self.by_tx.contains_key(&id) || !self.can_insert(id, &a) {
            return false;
        }
        let unit = a.candidate.key();
        let key = (a.deadline, unit, id);
        self.ordered.insert(key, a.candidate);
        self.by_tx.insert(id, key);
        self.bytes += a.bytes;
        self.pool_bytes += a.pool_bytes;
        self.sizes.insert(id, a.bytes);
        self.pool_sizes.insert(id, a.pool_bytes);
        *self.by_unit.entry(unit).or_default() += 1;
        true
    }
    pub fn remove(&mut self, id: &TransactionId) {
        if let Some(key) = self.by_tx.remove(id) {
            self.ordered.remove(&key);
            self.bytes -= self.sizes.remove(id).expect("indexed native size");
            self.pool_bytes -= self.pool_sizes.remove(id).expect("indexed native pool size");
            let count = self.by_unit.get_mut(&key.1).expect("indexed native unit");
            *count -= 1;
            if *count == 0 {
                self.by_unit.remove(&key.1);
            }
        }
    }
    pub fn entries(&self) -> impl Iterator<Item = (TransactionId, &NativeCarrierCandidateV22)> {
        self.ordered.iter().map(|(key, c)| (key.2, c))
    }
    pub fn contains(&self, id: &TransactionId) -> bool {
        self.by_tx.contains_key(id)
    }
    pub fn pool_occupation(&self) -> (usize, usize) {
        (self.by_tx.len(), self.pool_bytes)
    }
    pub fn update_pool_bytes(&mut self, id: &TransactionId, bytes: usize) {
        let Some(previous) = self.pool_sizes.get(id).copied() else { return };
        if bytes > *self.sizes.get(id).expect("indexed native charged size") {
            self.remove(id);
            return;
        }
        self.pool_bytes = self.pool_bytes - previous + bytes;
        self.pool_sizes.insert(*id, bytes);
    }
    pub fn same_unit(&self, id: &TransactionId, candidate: &NativeCarrierCandidateV22) -> bool {
        self.by_tx.get(id).is_some_and(|key| key.1 == candidate.key())
    }
    pub fn aliases(&self, candidate: &NativeCarrierCandidateV22) -> impl Iterator<Item = TransactionId> + '_ {
        let unit = candidate.key();
        self.ordered.iter().filter_map(move |(key, _)| (key.1 == unit).then_some(key.2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_flat_execution_capacity_v22::NativePaidCarrierRequestV22;
    use kaspa_hashes::Hash64;
    fn admission(namespace: u8, pending: u8, deadline: u64) -> NativeCarrierAdmissionV22 {
        NativeCarrierAdmissionV22 {
            candidate: NativeCarrierCandidateV22 {
                namespace,
                request: NativePaidCarrierRequestV22::SourceAnswer { pending: [pending; 64] },
                owner: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                    id(7),
                    0,
                )),
            },
            deadline,
            bytes: 100,
            pool_bytes: 100,
        }
    }
    fn id(n: u8) -> TransactionId {
        Hash64::from_bytes([n; 64])
    }
    #[test]
    fn original_unit_aliases_are_bounded_and_removal_releases_exactly_one() {
        let mut i = NativeCarrierIndexV22::new(4, 400);
        assert!(i.insert(id(1), admission(63, 1, 10)));
        assert!(i.insert(id(2), admission(63, 1, 10)));
        assert!(!i.insert(id(3), admission(63, 1, 10)), "a third UTXO copy gains no indexed position");
        assert!(
            i.can_insert_replacing(id(3), &admission(63, 1, 10), vec![id(1), id(1)].into_iter()),
            "genuine same-unit fee replacement releases one original alias once"
        );
        assert!(
            !i.can_insert_replacing(id(3), &admission(63, 1, 10), vec![id(99)].into_iter()),
            "unindexed replacement does not grant a third paid alias"
        );
        assert!(i.insert(id(4), admission(64, 1, 9)), "a separate namespace is a separate unit");
        assert!(i.insert(id(5), admission(63, 2, 8)));
        assert!(!i.insert(id(6), admission(63, 3, 7)), "bounded global metadata");
        assert_eq!(i.entries().map(|(id, _)| id).collect::<Vec<_>>(), vec![id(5), id(4), id(1), id(2)]);
        i.remove(&id(1));
        i.remove(&id(1));
        assert!(i.insert(id(3), admission(63, 1, 10)));
        let candidate = admission(63, 1, 10).candidate;
        assert_eq!(i.aliases(&candidate).collect::<Vec<_>>(), vec![id(2), id(3)]);
        let mut byte_limited = NativeCarrierIndexV22::new(4, 200);
        assert!(byte_limited.insert(id(1), admission(63, 1, 10)));
        assert!(byte_limited.insert(id(2), admission(63, 2, 10)));
        assert!(!byte_limited.insert(id(3), admission(63, 3, 10)), "resident bytes bind even with count room");
        byte_limited.remove(&id(1));
        assert!(byte_limited.insert(id(3), admission(63, 3, 10)));
    }
}
