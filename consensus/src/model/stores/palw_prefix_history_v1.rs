//! Paid public material archive independent of block-body/delta pruning. A rooted court
//! head selects content-addressed records, including after reorg. Never stores captures.
use super::*;
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
    palw_prefix_history_v1::{
        PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1, PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1, PalwStagedCacheLeaseV1,
    },
    palw_state_v2::PalwDeltaEntryV2,
};
use kaspa_database::prelude::{DbKey, DbWriter};
use rocksdb::IterateBounds;

// Tip is an exact singleton key. Its separator subspace is unused by that item and
// disjoint from the legacy assembly index under the schema singleton.
fn prefix(kind: u8) -> Vec<u8> {
    vec![DatabaseStorePrefixes::PalwStateV2Tip.into(), kaspa_database::registry::SEPARATOR, b'P', b'H', 1, kind]
}
fn record_key(head: Hash64) -> DbKey {
    DbKey::new(&prefix(b'r'), head)
}
fn expiry_key(until: u64, head: Hash64) -> Vec<u8> {
    let mut raw = prefix(b'e');
    raw.extend_from_slice(&until.to_be_bytes());
    raw.extend_from_slice(head.as_byte_slice());
    raw
}
fn cache_lease_key(seed: Hash64) -> DbKey {
    DbKey::new(&prefix(b'l'), seed)
}
fn cache_expiry_key(until: u64, seed: Hash64) -> Vec<u8> {
    let mut key = prefix(b'x');
    key.extend_from_slice(&until.to_be_bytes());
    key.extend_from_slice(seed.as_byte_slice());
    key
}
impl DbPalwStateV2Store {
    pub(super) fn clear_prefix_history_v1(&self, writer: &mut impl DbWriter) -> StoreResult<()> {
        for kind in [b'r', b'e', b'l', b'x'] {
            let (first, last) = rocksdb::PrefixRange(prefix(kind)).into_bounds();
            writer.delete_range(first.expect("archive start"), last.expect("archive end"))?;
        }
        Ok(())
    }
    pub fn staged_cache_lease_v1(&self, seed: Hash64) -> StoreResult<Option<PalwStagedCacheLeaseV1>> {
        let Some(raw) = self.db.get_pinned(cache_lease_key(seed))? else { return Ok(None) };
        if raw.len() > 512 {
            return Err(StoreError::DataInconsistency("native cache lease exceeds fixed metadata bound".into()));
        }
        let lease: PalwStagedCacheLeaseV1 =
            borsh::from_slice(&raw).map_err(|e| StoreError::DataInconsistency(e.to_string()))?;
        lease.check_v1().map_err(StoreError::DataInconsistency)?;
        if lease.seed_id != seed {
            return Err(StoreError::DataInconsistency("native cache lease changes its indexed seed".into()));
        }
        Ok(Some(lease))
    }
    /// Atomic with the state tip. No wire request or local manifest can install a
    /// lease, and a reverted acceptance cannot shorten another fork's retention.
    pub fn archive_staged_cache_leases_batch_v1(&self, batch: &mut WriteBatch, delta: &PalwStateDeltaV2) -> StoreResult<()> {
        let mut leases = std::collections::BTreeMap::<Hash64, PalwStagedCacheLeaseV1>::new();
        for entry in &delta.entries {
            let PalwDeltaEntryV2::StagedCourtSeed { new: Some(row), .. } = entry else { continue };
            for lease in row.public_cache_leases_v1() {
                lease.check_v1().map_err(StoreError::DataInconsistency)?;
                if let Some(previous) = leases.get_mut(&lease.seed_id) {
                    previous.merge_accepted_fork_v1(lease).map_err(StoreError::DataInconsistency)?;
                } else {
                    leases.insert(lease.seed_id, lease);
                }
            }
        }
        self.write_staged_cache_leases_batch_v1(batch, &leases)
    }
    fn write_staged_cache_leases_batch_v1(
        &self,
        batch: &mut WriteBatch,
        leases: &std::collections::BTreeMap<Hash64, PalwStagedCacheLeaseV1>,
    ) -> StoreResult<()> {
        for (&seed, &accepted) in leases {
            accepted.check_v1().map_err(StoreError::DataInconsistency)?;
            if seed != accepted.seed_id {
                return Err(StoreError::DataInconsistency("accepted cache lease changes its batch key".into()));
            }
            let mut lease = accepted;
            if let Some(previous) = self.staged_cache_lease_v1(seed)? {
                lease.merge_accepted_fork_v1(previous).map_err(StoreError::DataInconsistency)?;
                if lease == previous {
                    continue;
                }
                batch.delete(cache_expiry_key(previous.material_until_daa, seed));
            }
            batch.put(cache_lease_key(seed), borsh::to_vec(&lease).map_err(|e| StoreError::DataInconsistency(e.to_string()))?);
            batch.put(cache_expiry_key(lease.material_until_daa, seed), []);
        }
        Ok(())
    }
    /// Scan at most64 rows, including skipped rows, to prevent malformed or stale
    /// expiry entries from buying an unbounded read. Only the native caller supplies pp.
    pub fn staged_cache_gc_candidates_v1(
        &self,
        pruning_daa: u64,
        after: Option<(u64, Hash64)>,
        limit: usize,
    ) -> StoreResult<(Vec<PalwStagedCacheLeaseV1>, Option<(u64, Hash64)>)> {
        let first = prefix(b'x');
        let seek = after.map(|(until, seed)| cache_expiry_key(until, seed)).unwrap_or_else(|| first.clone());
        let mut iterator = self.db.raw_iterator();
        iterator.seek(&seek);
        if after.is_some() && iterator.key().is_some_and(|key| key == seek.as_slice()) {
            iterator.next();
        }
        let mut leases = Vec::new();
        let mut next = None;
        let mut visited = 0usize;
        while iterator.valid() {
            let Some(key) = iterator.key() else { break };
            if !key.starts_with(&first) {
                break;
            }
            if key.len() != first.len() + 8 + 64 {
                return Err(StoreError::DataInconsistency("cache lease expiry index has noncanonical key".into()));
            }
            let until = u64::from_be_bytes(key[first.len()..first.len() + 8].try_into().expect("8"));
            if until >= pruning_daa {
                break;
            }
            if visited >= limit.clamp(1, 64) {
                iterator.status()?;
                return Ok((leases, next));
            }
            let seed = Hash64::from_bytes(key[first.len() + 8..].try_into().expect("64"));
            visited += 1;
            next = Some((until, seed));
            if let Some(lease) = self.staged_cache_lease_v1(seed)? {
                if lease.material_until_daa != until {
                    return Err(StoreError::DataInconsistency("cache lease expiry differs from accepted retention union".into()));
                }
                leases.push(lease);
            }
            iterator.next();
        }
        iterator.status()?;
        Ok((leases, None))
    }
    /// Remove metadata only after local bounded cleanup acknowledges the exact
    /// lease. A subsequent accepted-fork union invalidates an older GC grant.
    pub fn acknowledge_staged_cache_gc_v1(&self, lease: PalwStagedCacheLeaseV1, pruning_daa: u64) -> StoreResult<bool> {
        lease.check_v1().map_err(StoreError::DataInconsistency)?;
        if pruning_daa <= lease.material_until_daa {
            return Ok(false);
        }
        let Some(current) = self.staged_cache_lease_v1(lease.seed_id)? else { return Ok(true) };
        if current != lease {
            return Ok(false);
        }
        let mut batch = WriteBatch::default();
        batch.delete(cache_lease_key(lease.seed_id));
        batch.delete(cache_expiry_key(lease.material_until_daa, lease.seed_id));
        self.db.write(batch)?;
        Ok(true)
    }
    pub fn insert_prefix_history_record_batch_v1(
        &self,
        batch: &mut WriteBatch,
        record: &PalwPrefixHistoryRecordV1,
    ) -> StoreResult<()> {
        let head = record.head_v1().map_err(StoreError::DataInconsistency)?;
        let raw = borsh::to_vec(record).map_err(|e| StoreError::DataInconsistency(e.to_string()))?;
        if raw.len() > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 {
            return Err(StoreError::DataInconsistency("public archive record exceeds its paid byte bound".into()));
        }
        batch.put(record_key(head), raw);
        batch.put(expiry_key(record.material_until_daa, head), []);
        Ok(())
    }
    /// Only a SUCCESSFUL native signed move's new rooted checkpoint may create a record.
    /// UTXO acceptance or a plausible peer carrier alone cannot claim court acceptance.
    pub fn archive_prefix_movements_batch_v1(
        &self,
        batch: &mut WriteBatch,
        delta: &PalwStateDeltaV2,
        movements: &[PalwConstraintStreamMoveV1],
    ) -> StoreResult<()> {
        for entry in &delta.entries {
            let PalwDeltaEntryV2::StagedCourtMove { key, old, new } = entry else { continue };
            let (Some(old), Some(new)) = (old.public_prefix_checkpoint_v1(), new.public_prefix_checkpoint_v1()) else { continue };
            let (old_count, old_head) = old.history_cursor_v1();
            let (count, head) = new.history_cursor_v1();
            // Automatic defaults/expiry have no signed movement and no new history link.
            if (count, head) == (old_count, old_head) {
                continue;
            }
            if old_count.checked_add(1) != Some(count) || head == old_head {
                return Err(StoreError::DataInconsistency("native public history checkpoint is not one exact link".into()));
            }
            let mut selected = None;
            for movement in movements.iter().filter(|m| (m.claim, m.reserver) == *key) {
                let Ok(candidate) = PalwPrefixHistoryRecordV1::new(
                    old_head,
                    count,
                    delta.point.block,
                    delta.point.daa_score,
                    new.history_material_until_v1(),
                    movement.clone(),
                ) else {
                    continue;
                };
                if candidate.head_v1().map_err(StoreError::DataInconsistency)? == head {
                    if selected.as_ref().is_some_and(|row| row != &candidate) {
                        return Err(StoreError::DataInconsistency("native history link has competing wire material".into()));
                    }
                    selected = Some(candidate);
                }
            }
            let record = selected.ok_or_else(|| {
                StoreError::DataInconsistency("successful public history link lacks its original accepted signed carrier".into())
            })?;
            self.insert_prefix_history_record_batch_v1(batch, &record)?;
        }
        Ok(())
    }
    pub fn prefix_history_record_v1(&self, request: &PalwPrefixHistoryRequestV1) -> StoreResult<Option<PalwPrefixHistoryRecordV1>> {
        request.check_v1().map_err(StoreError::DataInconsistency)?;
        let Some(raw) = self.db.get_pinned(record_key(request.head))? else { return Ok(None) };
        if raw.len() > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 {
            return Err(StoreError::DataInconsistency("public history record exceeds bounded read".into()));
        }
        let record: PalwPrefixHistoryRecordV1 = borsh::from_slice(&raw)
            .map_err(|e| StoreError::DataInconsistency(format!("public archive record does not decode: {e}")))?;
        record.authenticates_request_v1(request).map_err(StoreError::DataInconsistency)?;
        Ok(Some(record))
    }
    /// Pruning point is irreversible. A losing candidate's future DAA or a tip clock
    /// that can roll back may NEVER expire another branch's still-live public material.
    pub fn gc_prefix_history_batch_v1(&self, batch: &mut WriteBatch, pruning_daa: u64, limit: usize) -> StoreResult<usize> {
        let first = prefix(b'e');
        let mut iterator = self.db.raw_iterator();
        iterator.seek(&first);
        let mut count = 0;
        while iterator.valid() && count < limit.min(64) {
            let Some(key) = iterator.key() else { break };
            if !key.starts_with(&first) {
                break;
            }
            if key.len() != first.len() + 8 + 64 {
                return Err(StoreError::DataInconsistency("public archive expiry index has noncanonical key".into()));
            }
            let until = u64::from_be_bytes(key[first.len()..first.len() + 8].try_into().expect("8"));
            if until >= pruning_daa {
                break;
            }
            let head = Hash64::from_bytes(key[first.len() + 8..].try_into().expect("64"));
            batch.delete(record_key(head));
            batch.delete(key);
            count += 1;
            iterator.next();
        }
        iterator.status()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1,
        palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
        palw_prefix_family_court_v1::PalwPrefixCourtActionV1,
        palw_state_v2::PalwBondKeyV2,
        tx::{TransactionId, TransactionOutpoint},
    };
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    fn record(block: u64) -> PalwPrefixHistoryRecordV1 {
        PalwPrefixHistoryRecordV1::new(
            Hash64::default(),
            1,
            BlockHash::from_u64_word(block),
            100,
            200,
            PalwConstraintStreamMoveV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(2), index: 0 }),
                reserved_daa: 90,
                seed_id: Hash64::from_u64_word(3),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::Prefix(PalwPrefixCourtActionV1::FinishTile),
                signature: vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
        )
        .unwrap()
    }
    fn query(row: &PalwPrefixHistoryRecordV1) -> PalwPrefixHistoryRequestV1 {
        PalwPrefixHistoryRequestV1 {
            claim: row.movement.claim,
            owner: row.movement.reserver,
            reserved_daa: row.movement.reserved_daa,
            seed_id: row.movement.seed_id,
            head: row.head_v1().unwrap(),
            sequence: row.sequence,
            material_until_daa: row.material_until_daa,
        }
    }
    #[test]
    fn paid_public_archive_survives_body_delta_pruning_and_reorg_selects_only_requested_head() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(64));
        store.reindex_if_stale().unwrap();
        let first = record(8);
        let fork = record(9);
        let mut batch = WriteBatch::default();
        store.insert_prefix_history_record_batch_v1(&mut batch, &first).unwrap();
        store.insert_prefix_history_record_batch_v1(&mut batch, &fork).unwrap();
        // This deletion is the real pruning writer. Archive keys are not per-block rows.
        store.delete_delta_batch(&mut batch, first.accepted_block).unwrap();
        db.write(batch).unwrap();
        let restored = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(64));
        assert_eq!(restored.prefix_history_record_v1(&query(&first)).unwrap(), Some(first.clone()));
        assert_eq!(restored.prefix_history_record_v1(&query(&fork)).unwrap(), Some(fork));
        let mut wrong = query(&first);
        wrong.seed_id = Hash64::from_u64_word(99);
        assert!(restored.prefix_history_record_v1(&wrong).is_err());
        let mut batch = WriteBatch::default();
        assert_eq!(restored.gc_prefix_history_batch_v1(&mut batch, 200, 64).unwrap(), 0);
        assert_eq!(restored.gc_prefix_history_batch_v1(&mut batch, 201, 1).unwrap(), 1);
        db.write(batch).unwrap();
        let mut batch = WriteBatch::default();
        assert_eq!(restored.gc_prefix_history_batch_v1(&mut batch, 201, 64).unwrap(), 1);
        db.write(batch).unwrap();
        assert!(restored.prefix_history_record_v1(&query(&first)).unwrap().is_none());
    }
    fn lease(seed: u64, until: u64) -> PalwStagedCacheLeaseV1 {
        PalwStagedCacheLeaseV1 {
            claim: Hash64::from_u64_word(1), owner: record(8).movement.reserver, reserved_daa: 90,
            seed_id: Hash64::from_u64_word(seed), material_until_daa: until,
        }
    }
    fn save_lease(store: &DbPalwStateV2Store, lease: PalwStagedCacheLeaseV1) {
        let mut batch = WriteBatch::default();
        store.write_staged_cache_leases_batch_v1(&mut batch, &[(lease.seed_id, lease)].into_iter().collect()).unwrap();
        store.db.write(batch).unwrap();
    }
    #[test]
    fn native_cache_lease_union_invalidates_stale_gc_grant_without_extending_court_clock() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(64));
        let old = lease(3, 200);
        save_lease(&store, old);
        assert!(store.staged_cache_gc_candidates_v1(200, None, 64).unwrap().0.is_empty());
        let grant = store.staged_cache_gc_candidates_v1(201, None, 64).unwrap().0[0];
        save_lease(&store, lease(3, 250));
        assert!(!store.acknowledge_staged_cache_gc_v1(grant, 251).unwrap());
        assert!(store.staged_cache_gc_candidates_v1(201, None, 64).unwrap().0.is_empty());
        save_lease(&store, old);
        let current = store.staged_cache_lease_v1(old.seed_id).unwrap().unwrap();
        assert_eq!(current.material_until_daa, 250);
        assert!(!store.acknowledge_staged_cache_gc_v1(current, 250).unwrap());
        assert!(store.acknowledge_staged_cache_gc_v1(current, 251).unwrap());
        assert!(store.acknowledge_staged_cache_gc_v1(current, 251).unwrap());
        assert!(store.staged_cache_lease_v1(old.seed_id).unwrap().is_none());
        // These are storage primitive tests. They do not claim signature/fee admission.
        assert_eq!(old.material_until_daa, 200);
    }
    #[test]
    fn native_cache_lease_gc_pages_include_skipped_entries_in_the_scan_budget() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(64));
        for seed in 1..=5 {
            save_lease(&store, lease(seed, 200));
        }
        // A stale missing metadata row still consumes one bounded scan slot.
        let mut batch = WriteBatch::default();
        batch.delete(cache_lease_key(Hash64::from_u64_word(1)));
        db.write(batch).unwrap();
        let mut after = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0;
        loop {
            let (leases, next) = store.staged_cache_gc_candidates_v1(201, after, 1).unwrap();
            assert!(leases.len() <= 1);
            for lease in leases { assert!(seen.insert(lease.seed_id)); }
            pages += 1;
            assert!(pages <= 5);
            if next.is_none() { break; }
            after = next;
        }
        assert_eq!(seen.len(), 4);
        assert_eq!(pages, 5);
        let mut changed = lease(3, 300);
        changed.claim = Hash64::from_u64_word(99);
        let mut batch = WriteBatch::default();
        assert!(store.write_staged_cache_leases_batch_v1(&mut batch,
            &[(changed.seed_id, changed)].into_iter().collect()).is_err());
        assert_eq!(store.staged_cache_lease_v1(changed.seed_id).unwrap().unwrap().material_until_daa, 200);
    }
}
