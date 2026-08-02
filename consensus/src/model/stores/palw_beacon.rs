use std::{collections::BTreeMap, sync::Arc};

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::palw::{BeaconEpochInputs, PalwBeaconEpochAccumV1, PalwBeaconStateV1};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResultExt;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter, StoreError};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use rocksdb::WriteBatch;

use super::U64Key;

/// Fork-local PALW beacon accumulator carried by each block.
///
/// The original scaffold stores one accumulator under a global epoch key. That is sufficient for
/// pure tests while PALW is inert, but it is not a consensus-safe activation format: a side branch
/// can write a commit first and leak it into the selected chain. This view is instead keyed by block
/// hash and inherited from the selected parent. Only the two look-ahead epochs normally remain:
/// commits target `P + 2`, reveals target `P + 1`, and epoch `P` is removed after deriving `R_P`.
///
/// `stake_by_epoch` freezes the active DNS-bond amount when a commit is accepted. Quorum therefore
/// does not change retroactively when the bond later unbonds/slashes, and no virtual-tip/global bond
/// lookup is needed at the seed boundary.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwBeaconAccumViewV1 {
    pub version: u16,
    pub epochs: BTreeMap<u64, PalwBeaconEpochAccumV1>,
    pub stake_by_epoch: BTreeMap<u64, Vec<(TransactionOutpoint, u64)>>,
}

impl PalwBeaconAccumViewV1 {
    pub fn new() -> Self {
        Self { version: 1, epochs: BTreeMap::new(), stake_by_epoch: BTreeMap::new() }
    }

    pub fn commitment_of(&self, epoch: u64, bond: &TransactionOutpoint) -> Option<Hash64> {
        self.epochs.get(&epoch).and_then(|a| a.commitment_of(bond))
    }

    /// First commit for `(epoch, bond)` wins and snapshots the bond amount at that exact transition.
    pub fn record_commit(&mut self, epoch: u64, bond: TransactionOutpoint, commitment: Hash64, stake: u64) -> bool {
        let accum = self.epochs.entry(epoch).or_default();
        if accum.commitment_of(&bond).is_some() {
            return false;
        }
        accum.record_commit(bond, commitment);
        self.stake_by_epoch.entry(epoch).or_default().push((bond, stake));
        true
    }

    pub fn record_valid_reveal(&mut self, epoch: u64, bond: TransactionOutpoint, reveal_value: Hash64) -> bool {
        let Some(accum) = self.epochs.get_mut(&epoch) else { return false };
        let before = accum.valid_reveals.len();
        accum.record_valid_reveal(bond, reveal_value);
        accum.valid_reveals.len() != before
    }

    pub fn epoch_inputs(&self, epoch: u64) -> BeaconEpochInputs {
        self.epochs.get(&epoch).map(PalwBeaconEpochAccumV1::to_inputs).unwrap_or_default()
    }

    pub fn stake_of(&self, epoch: u64, bond: &TransactionOutpoint) -> u128 {
        self.stake_by_epoch
            .get(&epoch)
            .and_then(|rows| rows.iter().find_map(|(b, amount)| (b == bond).then_some(*amount as u128)))
            .unwrap_or(0)
    }

    /// Drop epochs that can no longer receive a valid commit/reveal after `current_epoch`'s seed was
    /// derived. Keeps only future targets, bounding the carried value independently of chain length.
    pub fn retain_future_of(&mut self, current_epoch: u64) {
        self.epochs.retain(|epoch, _| *epoch > current_epoch);
        self.stake_by_epoch.retain(|epoch, _| *epoch > current_epoch);
    }
}

impl Default for PalwBeaconAccumViewV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwBeaconAccumViewV1 {
    fn estimate_mem_units(&self) -> usize {
        self.epochs.values().map(|a| a.commits.len() + a.valid_reveals.len()).sum::<usize>().max(1)
    }
}

/// kaspa-pq ADR-0039 PALW (§11.2 / §18.1) — the DNS beacon overlay store. Three access paths:
///
/// * `accum` (prefix 242, keyed by **epoch**): the per-epoch [`PalwBeaconEpochAccumV1`] — the
///   `(bond, commitment)` sets that committed for the epoch and the subset that validly revealed
///   (`matches_commit`). Retained only as a pre-activation/diagnostic compatibility path; activated
///   networks never read or dual-write it.
/// * `accum_by_block` (prefix 244, keyed by **block hash**): the activated fork-local accumulator.
///   A child clones only its selected parent's view, so side branches and processing order cannot
///   contaminate an epoch seed.
/// * `state` (prefix 243, keyed by **block hash**): the block's carried [`PalwBeaconStateV1`] — the
///   active `R_E` recurrence. Block-keyed and read via the selected parent (`reserve_balance` pattern),
///   so it is past-relative + reorg-safe and forward-compatible with the eventual header commitment.
///
/// **Inert (never written)** while the PALW fence is `activation_daa_score = u64::MAX`; the block-keyed
/// paths are exercised only on a PALW-activated hard-fork/re-genesis network.
#[derive(Clone)]
pub struct DbPalwBeaconStore {
    db: Arc<DB>,
    accum: CachedDbAccess<U64Key, Arc<PalwBeaconEpochAccumV1>>,
    accum_by_block: CachedDbAccess<BlockHash, Arc<PalwBeaconAccumViewV1>, BlockHasher>,
    state: CachedDbAccess<BlockHash, Arc<PalwBeaconStateV1>, BlockHasher>,
    /// ADR-0045 D3-a — `epoch -> R_E`, the per-epoch seed HISTORY (prefix 67).
    seed_history: CachedDbAccess<U64Key, Arc<Hash64>>,
}

impl DbPalwBeaconStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            accum: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwBeaconAccum.into()),
            accum_by_block: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwBeaconAccumByBlock.into()),
            state: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwBeaconState.into()),
            seed_history: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwBeaconSeedHistory.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    // ---- epoch accumulator (commit/reveal facts) ----

    fn read_accum(&self, epoch: u64) -> Result<PalwBeaconEpochAccumV1, StoreError> {
        Ok(self.accum.read(epoch.into()).optional()?.map(|a| (*a).clone()).unwrap_or_default())
    }

    /// Record a beacon commit for `bond` in `epoch` (read-modify-write; idempotent per bond outpoint).
    pub fn record_commit(&self, epoch: u64, bond: TransactionOutpoint, commitment: Hash64) -> Result<(), StoreError> {
        let mut accum = self.read_accum(epoch)?;
        accum.record_commit(bond, commitment);
        self.accum.write(DirectDbWriter::new(&self.db), epoch.into(), Arc::new(accum))
    }

    /// The commitment `bond` committed for `epoch`, if any — used to check a reveal's `matches_commit`.
    pub fn commitment_of(&self, epoch: u64, bond: &TransactionOutpoint) -> Result<Option<Hash64>, StoreError> {
        Ok(self.read_accum(epoch)?.commitment_of(bond))
    }

    /// Record that `bond`'s reveal validly opened `commitment` in `epoch` (idempotent per bond outpoint).
    pub fn record_valid_reveal(&self, epoch: u64, bond: TransactionOutpoint, commitment: Hash64) -> Result<(), StoreError> {
        let mut accum = self.read_accum(epoch)?;
        accum.record_valid_reveal(bond, commitment);
        self.accum.write(DirectDbWriter::new(&self.db), epoch.into(), Arc::new(accum))
    }

    /// The pure derivation inputs for `epoch` (empty if nothing accumulated).
    pub fn epoch_inputs(&self, epoch: u64) -> Result<BeaconEpochInputs, StoreError> {
        Ok(self.read_accum(epoch)?.to_inputs())
    }

    pub fn delete_accum_batch(&self, batch: &mut WriteBatch, epoch: u64) -> Result<(), StoreError> {
        self.accum.delete(BatchDbWriter::new(batch), epoch.into())
    }

    // ---- fork-local accumulator view (activation path) ----

    /// The accumulator carried by `block`, or `None` at genesis / before activation.
    pub fn accum_view(&self, block: BlockHash) -> Result<Option<Arc<PalwBeaconAccumViewV1>>, StoreError> {
        self.accum_by_block.read(block).optional()
    }

    pub fn set_accum_view_batch(
        &self,
        batch: &mut WriteBatch,
        block: BlockHash,
        view: Arc<PalwBeaconAccumViewV1>,
    ) -> Result<(), StoreError> {
        self.accum_by_block.write(BatchDbWriter::new(batch), block, view)
    }

    pub fn set_accum_view(&self, block: BlockHash, view: Arc<PalwBeaconAccumViewV1>) -> Result<(), StoreError> {
        self.accum_by_block.write(DirectDbWriter::new(&self.db), block, view)
    }

    pub fn delete_accum_view_batch(&self, batch: &mut WriteBatch, block: BlockHash) -> Result<(), StoreError> {
        self.accum_by_block.delete(BatchDbWriter::new(batch), block)
    }

    // ---- per-block carried state (R_E recurrence) ----

    /// The block's carried beacon state, or `None` if absent (genesis / not yet derived).
    pub fn beacon_state(&self, block: BlockHash) -> Result<Option<Arc<PalwBeaconStateV1>>, StoreError> {
        self.state.read(block).optional()
    }

    /// Write `block`'s carried beacon state into the commit `WriteBatch` (atomic with the UTXO diff).
    pub fn set_state_batch(&self, batch: &mut WriteBatch, block: BlockHash, state: Arc<PalwBeaconStateV1>) -> Result<(), StoreError> {
        self.state.write(BatchDbWriter::new(batch), block, state)
    }

    /// Direct (non-batch) beacon-state write — diagnostics / tests.
    pub fn set_state(&self, block: BlockHash, state: Arc<PalwBeaconStateV1>) -> Result<(), StoreError> {
        self.state.write(DirectDbWriter::new(&self.db), block, state)
    }

    pub fn delete_state_batch(&self, batch: &mut WriteBatch, block: BlockHash) -> Result<(), StoreError> {
        self.state.delete(BatchDbWriter::new(batch), block)
    }

    // ---- per-epoch seed history (ADR-0045 D3-a) ----

    /// `R_E` for `epoch`, or `None` when the epoch is outside the retained window (or predates
    /// activation). **`None` is fail-closed by contract**: a PCPB check that cannot resolve the
    /// epoch its ticket was bound to must refuse the ticket, never fall back to a current seed —
    /// that fallback is precisely the grindable state ADR-0045 D3-b forbids.
    pub fn beacon_seed_at(&self, epoch: u64) -> Result<Option<Hash64>, StoreError> {
        Ok(self.seed_history.read(epoch.into()).optional()?.map(|seed| *seed))
    }

    /// Record `epoch`'s derived seed, atomically with the block that closed the epoch.
    ///
    /// Idempotent-by-value rather than write-once: a reorg past an epoch boundary legitimately
    /// re-resolves the same epoch from the NEW selected chain, and the per-epoch value is a
    /// function of "the chain that first closed this epoch", so the new boundary block's value is
    /// the correct one. Overwriting is the reorg-reversibility the block-keyed stores get for free
    /// by being keyed on a hash that simply stops being read.
    pub fn set_beacon_seed_batch(&self, batch: &mut WriteBatch, epoch: u64, seed: Hash64) -> Result<(), StoreError> {
        self.seed_history.write(BatchDbWriter::new(batch), epoch.into(), Arc::new(seed))
    }

    pub fn delete_beacon_seed_batch(&self, batch: &mut WriteBatch, epoch: u64) -> Result<(), StoreError> {
        self.seed_history.delete(BatchDbWriter::new(batch), epoch.into())
    }

    /// Every retained `(epoch, R_E)` row in ascending epoch order — the pruning-snapshot carry and
    /// the bounded-window audit read this.
    pub fn beacon_seed_history(&self) -> Result<Vec<(u64, Hash64)>, StoreError> {
        let mut rows = Vec::new();
        for entry in self.seed_history.iterator() {
            let (key, seed) = entry.map_err(|err| StoreError::DataInconsistency(format!("beacon seed history scan: {err}")))?;
            let bytes: [u8; 8] =
                key.as_ref().try_into().map_err(|_| StoreError::DataInconsistency("beacon seed history key width".into()))?;
            // `U64Key` is little-endian, so the raw key order is NOT epoch order — hence the
            // explicit sort below rather than relying on the iterator.
            rows.push((u64::from_le_bytes(bytes), *seed));
        }
        rows.sort_unstable_by_key(|(epoch, _)| *epoch);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn op(b: u8, i: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([b; 64]), i)
    }
    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    /// ADR-0045 D3-a — the seed history answers past epochs, is overwritable by a reorg, and
    /// falls closed outside the window.
    ///
    /// The three pre-existing sources of `R_E` cannot serve PCPB: the header walk fails closed
    /// below the pruning point, per-block state rows are deleted by the pruning pass, and the
    /// accumulator's `retain_future_of` (asserted below) drops past epochs by design because it
    /// carries the seed's MATERIAL, not its value. This store is the value, retained.
    #[test]
    fn beacon_seed_history_round_trips_sweeps_and_falls_closed() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwBeaconStore::new(db.clone(), CachePolicy::Empty);

        // The gap this store exists to fill: the accumulator forgets past epochs on purpose.
        let mut accum = PalwBeaconAccumViewV1::new();
        accum.record_commit(5, op(1, 0), h(0x55), 10);
        accum.record_commit(9, op(1, 0), h(0x99), 10);
        accum.retain_future_of(7);
        assert!(accum.epochs.contains_key(&9) && !accum.epochs.contains_key(&5), "past epochs are dropped by design");

        // An unwritten epoch is `None` — PCPB fails closed rather than substituting a live seed.
        assert_eq!(store.beacon_seed_at(5).unwrap(), None);

        let mut batch = WriteBatch::default();
        for (epoch, seed) in [(5u64, h(0x55)), (6, h(0x66)), (7, h(0x77))] {
            store.set_beacon_seed_batch(&mut batch, epoch, seed).unwrap();
        }
        db.write(batch).unwrap();
        assert_eq!(store.beacon_seed_at(5).unwrap(), Some(h(0x55)));
        assert_eq!(store.beacon_seed_at(7).unwrap(), Some(h(0x77)));
        assert_eq!(store.beacon_seed_history().unwrap(), vec![(5, h(0x55)), (6, h(0x66)), (7, h(0x77))]);

        // Reorg: a new selected chain re-closes epoch 7, and its value is the correct one. The
        // block-keyed stores get this for free by keying on a hash nobody reads again; an
        // epoch-keyed row has to be overwritten explicitly.
        let mut batch = WriteBatch::default();
        store.set_beacon_seed_batch(&mut batch, 7, h(0x7E)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.beacon_seed_at(7).unwrap(), Some(h(0x7E)));

        // Sweeping the window floor removes older rows and leaves the rest intact.
        let mut batch = WriteBatch::default();
        store.delete_beacon_seed_batch(&mut batch, 5).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.beacon_seed_at(5).unwrap(), None, "swept epochs fall closed, not stale");
        assert_eq!(store.beacon_seed_history().unwrap(), vec![(6, h(0x66)), (7, h(0x7E))]);
    }

    #[test]
    fn default_accumulator_view_is_canonical_v1() {
        assert_eq!(PalwBeaconAccumViewV1::default(), PalwBeaconAccumViewV1::new());
    }

    /// The epoch accumulator: commits accumulate, a matching reveal is recorded, epoch_inputs reflects
    /// both, and the whole thing round-trips through the DB. Idempotent per bond.
    #[test]
    fn accum_commit_reveal_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwBeaconStore::new(db, CachePolicy::Count(16));

        // empty epoch → empty inputs.
        assert_eq!(store.epoch_inputs(9).unwrap(), BeaconEpochInputs::default());

        // two commits; a duplicate on the same bond is ignored.
        store.record_commit(9, op(0x50, 0), h(11)).unwrap();
        store.record_commit(9, op(0x50, 1), h(12)).unwrap();
        store.record_commit(9, op(0x50, 0), h(99)).unwrap();
        assert_eq!(store.commitment_of(9, &op(0x50, 0)).unwrap(), Some(h(11)));
        let inputs = store.epoch_inputs(9).unwrap();
        assert_eq!(inputs.commits.len(), 2);
        assert_eq!(inputs.valid_reveals.len(), 0);

        // record one valid reveal.
        store.record_valid_reveal(9, op(0x50, 0), h(11)).unwrap();
        let inputs = store.epoch_inputs(9).unwrap();
        assert_eq!(inputs.valid_reveals, vec![(op(0x50, 0), h(11))]);
        // a different epoch is independent.
        assert_eq!(store.epoch_inputs(10).unwrap(), BeaconEpochInputs::default());
    }

    /// The per-block state store: absent → None, set → read back, batch write + delete.
    #[test]
    fn state_block_keyed_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwBeaconStore::new(db, CachePolicy::Count(16));
        let block = h(0x21);
        assert!(store.beacon_state(block).unwrap().is_none());

        let st = Arc::new(PalwBeaconStateV1 {
            version: 1,
            closed_seeds: Vec::new(),
            prev_closer: None,
            epoch: 9,
            seed: h(1),
            dns_anchor: h(2),
            anchor_blue_score: 20,
            anchor_daa_score: 21,
            anchor_overlay_root: h(22),
            valid_reveals_root: h(3),
            missing_commitments_root: h(4),
            mode: 0,
            degraded_epochs: 0,
            valid_reveal_count: 1,
            missing_commit_count: 0,
        });
        let mut batch = WriteBatch::default();
        store.set_state_batch(&mut batch, block, st.clone()).unwrap();
        db_write(&store, batch);
        assert_eq!(*store.beacon_state(block).unwrap().unwrap(), *st);

        let mut batch = WriteBatch::default();
        store.delete_state_batch(&mut batch, block).unwrap();
        db_write(&store, batch);
        assert!(store.beacon_state(block).unwrap().is_none());
    }

    /// A child inherits only its selected parent's accumulator. Sibling writes remain disjoint, and
    /// the stake recorded with a commit is frozen in that fork-local value.
    #[test]
    fn block_accumulator_is_fork_local_and_freezes_stake() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwBeaconStore::new(db, CachePolicy::Count(16));
        let parent = h(0x30);
        let left = h(0x31);
        let right = h(0x32);
        let bond = op(0x60, 1);

        let mut base = PalwBeaconAccumViewV1::new();
        assert!(base.record_commit(9, bond, h(1), 30));
        assert!(!base.record_commit(9, bond, h(2), 999), "first commit/stake snapshot wins");
        store.set_accum_view(parent, Arc::new(base)).unwrap();

        let mut l = (*store.accum_view(parent).unwrap().unwrap()).clone();
        assert!(l.record_valid_reveal(9, bond, h(3)));
        store.set_accum_view(left, Arc::new(l)).unwrap();

        let mut r = (*store.accum_view(parent).unwrap().unwrap()).clone();
        assert!(r.record_commit(10, op(0x61, 0), h(4), 70));
        store.set_accum_view(right, Arc::new(r)).unwrap();

        let left_view = store.accum_view(left).unwrap().unwrap();
        let right_view = store.accum_view(right).unwrap().unwrap();
        assert_eq!(left_view.stake_of(9, &bond), 30);
        assert_eq!(left_view.epoch_inputs(9).valid_reveals.len(), 1);
        assert_eq!(right_view.epoch_inputs(9).valid_reveals.len(), 0, "left reveal must not leak to sibling");
        assert_eq!(right_view.stake_of(10, &op(0x61, 0)), 70);
    }

    fn db_write(store: &DbPalwBeaconStore, batch: WriteBatch) {
        store.db.write(batch).unwrap();
    }
}
