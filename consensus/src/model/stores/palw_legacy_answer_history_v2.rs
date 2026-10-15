//! Derived, node-local history index. Its bytes already occurred in an authenticated DA
//! assembly; it never supplies a producer capture or changes the consensus journal/root.
//! Keep only removal rows one part short, indexed by the missing part. The accepted last
//! carrier can reconstruct the original envelope without decoding the whole block delta.
use super::*;
use kaspa_consensus_core::palw_legacy_answer_chunks_v2::{
    PALW_LEGACY_ANSWER_MAX_BYTES_V2, PalwLegacyAnswerChunkV2, PalwPendingLegacyAnswerV2,
};
use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;
use kaspa_database::prelude::{DbKey, DbWriter};
use rocksdb::IterateBounds;

// The schema singleton uses the exact one-byte key. Its separator subspace is unused by
// that singleton and avoids mixing differently encoded rows into the delta iterator.
fn prefix(block: Option<BlockHash>) -> Vec<u8> {
    let mut bytes = vec![DatabaseStorePrefixes::PalwStateV2Schema.into(), kaspa_database::registry::SEPARATOR];
    if let Some(block) = block {
        bytes.extend_from_slice(block.as_byte_slice());
    }
    bytes
}

// 32 digests, the declaration's fixed MLDSA signature and bounded map/key framing.
const INDEX_HEADER_BYTES: u64 = 16_384;

fn key(block: BlockHash, chunk: &PalwLegacyAnswerChunkV2) -> DbKey {
    DbKey::new(&prefix(Some(block)), borsh::to_vec(&(chunk.key(), chunk.answer_digest, chunk.index)).expect("history key"))
}

impl DbPalwStateV2Store {
    pub(super) fn clear_legacy_answer_history(&self, writer: &mut impl DbWriter, block: Option<BlockHash>) -> StoreResult<()> {
        let (first, last) = rocksdb::PrefixRange(prefix(block)).into_bounds();
        writer.delete_range(first.expect("index prefix start"), last.expect("index prefix end"))?;
        Ok(())
    }

    pub(super) fn index_legacy_answer_history_batch(
        &self,
        batch: &mut WriteBatch,
        block: BlockHash,
        delta: &PalwStateDeltaV2,
    ) -> StoreResult<()> {
        self.clear_legacy_answer_history(&mut BatchDbWriter::new(batch), Some(block))?;
        for entry in &delta.entries {
            let PalwDeltaEntryV2::LegacyAnswer { key: owner, old: Some(row), new: None } = entry else { continue };
            if row.declaration.key() != *owner
                || row.parts.len() + 1 != row.declaration.chunk_digests.len()
                || row.check(PALW_LEGACY_ANSWER_MAX_BYTES_V2).is_err()
                || borsh::object_length(row).map_or(true, |n| n as u64 > PALW_LEGACY_ANSWER_MAX_BYTES_V2 + INDEX_HEADER_BYTES)
            {
                continue;
            }
            let Some(index) = row.progress().next_index() else { continue };
            let query = PalwLegacyAnswerChunkV2 {
                claim: owner.0,
                unit: owner.1,
                discloser: owner.2,
                answer_digest: row.declaration.answer_digest,
                index,
                bytes: Vec::new(),
            };
            batch.put(key(block, &query), borsh::to_vec(row).expect("accepted assembly row"));
        }
        Ok(())
    }

    /// A target-sized removal row, not the block's complete delta. Missing old derived indexes
    /// (pre-upgrade history) return None and use reacquisition. A missing block journal or a
    /// corrupt index is an error; neither can silently complete a history watermark.
    pub fn legacy_answer_history_row_v2(
        &self,
        block: BlockHash,
        chunk: &PalwLegacyAnswerChunkV2,
        maximum: u64,
    ) -> StoreResult<Option<PalwPendingLegacyAnswerV2>> {
        if !self.has_delta(block)? {
            return Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::PalwStateV2Deltas.as_ref(), block)));
        }
        let Some(raw) = self.db.get_pinned(key(block, chunk))? else { return Ok(None) };
        let maximum = maximum.min(PALW_LEGACY_ANSWER_MAX_BYTES_V2);
        if raw.len() as u64 > maximum.saturating_add(INDEX_HEADER_BYTES) {
            return Err(StoreError::DataInconsistency("legacy answer history index exceeds its read budget".into()));
        }
        let row: PalwPendingLegacyAnswerV2 = borsh::from_slice(&raw)
            .map_err(|e| StoreError::DataInconsistency(format!("legacy answer history index does not decode: {e}")))?;
        row.check(maximum).map_err(StoreError::DataInconsistency)?;
        if row.declaration.key() != chunk.key()
            || row.declaration.answer_digest != chunk.answer_digest
            || row.parts.len() + 1 != row.declaration.chunk_digests.len()
            || row.progress().next_index() != Some(chunk.index)
        {
            return Err(StoreError::DataInconsistency("legacy answer history index differs from its key".into()));
        }
        Ok(Some(row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_legacy_answer_chunks_v2::{PalwLegacyAnswerDeclarationV2, palw_legacy_answer_chunk_digest_v2};
    use kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2;
    use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2};
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_database::{
        create_temp_db,
        prelude::{ConnBuilder, DirectDbWriter},
    };

    // Synthetic journal rows test persistence/budgets only, not consensus or MLDSA acceptance.
    fn fixture() -> (BlockHash, PalwStateDeltaV2, PalwPendingLegacyAnswerV2, PalwLegacyAnswerChunkV2) {
        let block = Hash64::from_u64_word(1);
        let declaration = PalwLegacyAnswerDeclarationV2 {
            version: 2,
            claim: Hash64::from_u64_word(2),
            unit: PalwLegacyHeldUnitV2::FpJob,
            discloser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0)),
            part_bytes: 3,
            total_bytes: 6,
            chunk_digests: vec![palw_legacy_answer_chunk_digest_v2(0, b"abc"), palw_legacy_answer_chunk_digest_v2(1, b"def")],
            answer_digest: Hash64::from_u64_word(4),
            signature: vec![7; 4627],
        };
        let chunk = PalwLegacyAnswerChunkV2 {
            claim: declaration.claim,
            unit: declaration.unit,
            discloser: declaration.discloser,
            answer_digest: declaration.answer_digest,
            index: 1,
            bytes: b"def".to_vec(),
        };
        let row = PalwPendingLegacyAnswerV2 { declaration, deadline_daa: 1000, parts: [(0, b"abc".to_vec())].into_iter().collect() };
        let delta = PalwStateDeltaV2 {
            point: PalwBlockContextV2 { block, daa_score: 100, blue_score: 10, subsidy: 0 },
            entries: vec![PalwDeltaEntryV2::LegacyAnswer { key: row.declaration.key(), old: Some(row.clone()), new: None }],
        };
        (block, delta, row, chunk)
    }

    #[test]
    fn the_target_index_survives_restart_without_decoding_the_whole_journal_and_is_deleted_with_its_block() {
        let (_dir, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
        let (block, delta, row, chunk) = fixture();
        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, block, Hash64::from_u64_word(9), &delta).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.legacy_answer_history_row_v2(block, &chunk, 6).unwrap(), Some(row.clone()));
        let restarted = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
        assert_eq!(restarted.legacy_answer_history_row_v2(block, &chunk, 6).unwrap(), Some(row.clone()));
        assert_eq!(restarted.iter_delta_blocks().collect::<Vec<_>>(), vec![block], "the index is outside the delta iterator");
        store
            .set_delta_record_for_tests(block, PalwStateDeltaRecordV2 { state_root: Hash64::from_u64_word(9), delta_borsh: vec![255] })
            .unwrap();
        assert!(store.delta_of(block).is_err());
        assert_eq!(store.legacy_answer_history_row_v2(block, &chunk, 6).unwrap(), Some(row), "no full journal decode on this read");
        let mut batch = WriteBatch::default();
        store.delete_delta_batch(&mut batch, block).unwrap();
        db.write(batch).unwrap();
        assert!(db.get_pinned(key(block, &chunk)).unwrap().is_none(), "pruning/reorg removes the derived bytes in the same batch");
        assert!(matches!(store.legacy_answer_history_row_v2(block, &chunk, 6), Err(StoreError::KeyNotFound(_))));
    }

    #[test]
    fn budget_corruption_replacement_and_schema_reindex_are_distinct_from_a_missing_old_index() {
        let (_dir, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
        let (block, mut delta, _row, chunk) = fixture();
        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, block, Hash64::from_u64_word(9), &delta).unwrap();
        db.write(batch).unwrap();
        assert!(matches!(store.legacy_answer_history_row_v2(block, &chunk, 5), Err(StoreError::DataInconsistency(_))));
        db.put(key(block, &chunk), vec![0; INDEX_HEADER_BYTES as usize + 7]).unwrap();
        assert!(matches!(store.legacy_answer_history_row_v2(block, &chunk, 6), Err(StoreError::DataInconsistency(_))));
        db.put(key(block, &chunk), [255]).unwrap();
        assert!(matches!(store.legacy_answer_history_row_v2(block, &chunk, 6), Err(StoreError::DataInconsistency(_))));
        delta.entries.clear();
        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, block, Hash64::from_u64_word(9), &delta).unwrap();
        db.write(batch).unwrap();
        assert!(
            store.legacy_answer_history_row_v2(block, &chunk, 6).unwrap().is_none(),
            "rewriting a block clears stale derived rows"
        );
        db.put(key(block, &chunk), [1]).unwrap();
        store.schema.write(DirectDbWriter::new(&db), &0).unwrap();
        store.reindex_if_stale().unwrap();
        assert!(db.get_pinned(key(block, &chunk)).unwrap().is_none());
    }

    #[test]
    fn the_full_32_part_index_obeys_the_complete_envelope_budget_before_reassembly() {
        use kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_CHUNK_BYTES_V2;
        let (_dir, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
        let (block, mut delta, mut row, mut chunk) = fixture();
        let bytes = vec![8; PALW_LEGACY_ANSWER_CHUNK_BYTES_V2 as usize];
        row.declaration.part_bytes = PALW_LEGACY_ANSWER_CHUNK_BYTES_V2;
        row.declaration.total_bytes = PALW_LEGACY_ANSWER_MAX_BYTES_V2;
        row.declaration.chunk_digests = (0..32).map(|i| palw_legacy_answer_chunk_digest_v2(i, &bytes)).collect();
        row.parts = (0..31).map(|i| (i, bytes.clone())).collect();
        chunk.index = 31;
        chunk.bytes = bytes;
        delta.entries = vec![PalwDeltaEntryV2::LegacyAnswer { key: row.declaration.key(), old: Some(row.clone()), new: None }];
        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, block, Hash64::from_u64_word(9), &delta).unwrap();
        db.write(batch).unwrap();
        let raw = db.get_pinned(key(block, &chunk)).unwrap().unwrap();
        assert!(raw.len() as u64 <= PALW_LEGACY_ANSWER_MAX_BYTES_V2 + INDEX_HEADER_BYTES);
        assert_eq!(store.legacy_answer_history_row_v2(block, &chunk, PALW_LEGACY_ANSWER_MAX_BYTES_V2).unwrap(), Some(row));
        assert!(
            matches!(
                store.legacy_answer_history_row_v2(block, &chunk, PALW_LEGACY_ANSWER_MAX_BYTES_V2 - 1),
                Err(StoreError::DataInconsistency(_)),
            ),
            "a court budget one byte short never reaches answer concatenation/parsing"
        );
    }
}
