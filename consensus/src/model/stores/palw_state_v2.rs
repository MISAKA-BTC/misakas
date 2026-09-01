//! MISAKA PALW V2 chain state on disk (ADR-0042 Decision 5, PR-08's store shape; the same rows
//! are ADR-0044 Unit C's substrate — the free-prompt candidate walk reads them through
//! `processes::palw_state_walk`, the sink writer maintains them through
//! `processes::palw_state_v2_sync`).
//!
//! # What is persisted, and why exactly this
//!
//! Two things, mirroring what `PalwStateBookV2` holds in memory:
//!
//! * **Per chain block:** the transition's `PalwStateDeltaV2` and the `state_root` it produced.
//!   The delta is the reorg primitive — `apply_delta_v2` / `revert_delta_v2` move a materialized
//!   state along the selected chain bit-exactly, verifying every replaced value — and the root
//!   is what a candidate comparison may cite without materializing anything.
//! * **One singleton tip:** the materialized `PalwChainStateV2` at the selected sink, as a
//!   `PalwStateCarriageV2` snapshot plus the chain block it stands at. Everything else is a fold
//!   of deltas from here.
//!
//! A candidate's V2 standing is therefore a function of THAT candidate's chain — walk deltas from
//! the tip's block to the candidate's chain point — never a read of the node's sink state, which
//! is the P0-4 partition this layout exists to make unrepresentable.
//!
//! # Encoding
//!
//! Row bodies are the consensus types' **Borsh bytes, verbatim**, wrapped in a thin serde record
//! (the `DbPalwCarriageStore` pattern). Borsh is the canonical encoding the state's own digests
//! are defined over; re-encoding through the store's serde path would create a second, unpinned
//! canonical form. Decode failures surface as `StoreError::DataInconsistency`, never as "absent".
//!
//! # Trust discipline on load
//!
//! `load_tip` rebuilds the state through `PalwStateCarriageV2::into_state` with the RECORDED
//! root demanded — the same refusal a peer-supplied pruning carriage gets. A snapshot this node
//! wrote is still a snapshot a disk could corrupt or a tool could edit, and a poisoned sink is
//! the worst object in the system precisely because everything else is a fold from it.
//!
//! # Schema
//!
//! One `u32` vouches for both column layouts. On mismatch [`DbPalwStateV2Store::reindex_if_stale`]
//! deletes rows AND tip together: undecodable rows read as absent, and an absent V2 state looks
//! exactly like "no PALW work matured" — a wrong answer shaped like a valid one, which fork
//! choice would act on. An emptied store forces the consumer to rebuild from its chain instead.

use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{
    PalwChainStateV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error,
};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

/// The layout the rows were written under. Bump on ANY change to the wrapped Borsh types or to
/// the record shapes below, so old rows are discarded for rebuilding instead of read as absent.
pub const PALW_STATE_V2_STORE_SCHEMA_VERSION: u32 = 1;

/// Per-chain-block row: the transition's outcome, exactly as the state machine reported it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwStateDeltaRecordV2 {
    /// `state_root()` of the state this block's transition produced. Carried beside the delta so
    /// ordering/telemetry can cite a candidate's root without materializing its state.
    pub state_root: Hash64,
    /// `PalwStateDeltaV2`, Borsh bytes verbatim.
    pub delta_borsh: Vec<u8>,
}

impl MemSizeEstimator for PalwStateDeltaRecordV2 {}

/// The singleton tip row: where the materialized state stands, and what it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwStateTipRecordV2 {
    /// The chain block the snapshot is AT (its transition already applied).
    pub block: BlockHash,
    /// `state_root()` of the snapshot — demanded back at load, so a corrupted snapshot refuses
    /// to become a sink.
    pub state_root: Hash64,
    /// `PalwStateCarriageV2`, Borsh bytes verbatim.
    pub carriage_borsh: Vec<u8>,
}

/// PALW V2 chain state rows: per-chain-block deltas + the materialized tip snapshot.
#[derive(Clone)]
pub struct DbPalwStateV2Store {
    db: Arc<DB>,
    deltas: CachedDbAccess<BlockHash, Arc<PalwStateDeltaRecordV2>>,
    tip: CachedDbItem<PalwStateTipRecordV2>,
    /// The state materialised AT the pruning point, kept beside the tip because it answers a
    /// different question: the tip says where this node's chain is, and this says what a peer
    /// starting from the pruning point must be handed. The tip is rewritten to the sink on every
    /// virtual walk, so it can never stand in for this — which is why every pruned IBD used to
    /// abort.
    pruning_snapshot: CachedDbItem<PalwStateTipRecordV2>,
    schema: CachedDbItem<u32>,
}

impl DbPalwStateV2Store {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            deltas: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::PalwStateV2Deltas.into()),
            tip: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::PalwStateV2Tip.into()),
            pruning_snapshot: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::PalwPruningPointState.into()),
            schema: CachedDbItem::new(db, DatabaseStorePrefixes::PalwStateV2Schema.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// If the stored schema is not this build's, delete rows and tip TOGETHER and stamp the new
    /// version — directly, not batched, because a store that stamped without clearing (or cleared
    /// without stamping) would vouch for rows it no longer understands.
    pub fn reindex_if_stale(&mut self) -> StoreResult<()> {
        let stored = match self.schema.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(PALW_STATE_V2_STORE_SCHEMA_VERSION) {
            return Ok(());
        }
        self.deltas.delete_all(DirectDbWriter::new(&self.db))?;
        match self.tip.remove(DirectDbWriter::new(&self.db)) {
            Ok(()) | Err(StoreError::KeyNotFound(_)) => {}
            Err(e) => return Err(e),
        }
        self.schema.write(DirectDbWriter::new(&self.db), &PALW_STATE_V2_STORE_SCHEMA_VERSION)
    }

    // --- deltas ---

    /// Stage one chain block's transition outcome. Written when the block's transition is
    /// computed on the selected chain; the batch is the caller's commit point.
    pub fn insert_delta_batch(
        &mut self,
        batch: &mut WriteBatch,
        block: BlockHash,
        state_root: Hash64,
        delta: &PalwStateDeltaV2,
    ) -> StoreResult<()> {
        let delta_borsh = borsh::to_vec(delta).expect("PalwStateDeltaV2 is borsh-serializable");
        self.deltas.write(BatchDbWriter::new(batch), block, Arc::new(PalwStateDeltaRecordV2 { state_root, delta_borsh }))
    }

    /// Stage removal of a block's row (it left the selected chain and its delta was reverted).
    pub fn delete_delta_batch(&mut self, batch: &mut WriteBatch, block: BlockHash) -> StoreResult<()> {
        self.deltas.delete(BatchDbWriter::new(batch), block)
    }

    pub fn has_delta(&self, block: BlockHash) -> StoreResult<bool> {
        self.deltas.has(block)
    }

    /// Every chain block that has a delta row, in store order. For assertions about the store as
    /// a whole — "this network wrote nothing" is a fact about the absence of rows, and only an
    /// iterator can say it.
    pub fn iter_delta_blocks(&self) -> impl Iterator<Item = BlockHash> + '_ {
        self.deltas.iterator().filter_map(|r| r.ok()).map(|(key, _)| {
            let mut bytes = [0u8; 64];
            bytes.copy_from_slice(&key);
            BlockHash::from_bytes(bytes)
        })
    }

    /// A block's recorded root without decoding its delta.
    pub fn state_root_of(&self, block: BlockHash) -> StoreResult<Hash64> {
        Ok(self.deltas.read(block)?.state_root)
    }

    /// A block's transition outcome, decoded. A row whose bytes no longer decode is named
    /// (`DataInconsistency`), never reported absent.
    pub fn delta_of(&self, block: BlockHash) -> StoreResult<(Hash64, PalwStateDeltaV2)> {
        let record = self.deltas.read(block)?;
        let delta = borsh::from_slice::<PalwStateDeltaV2>(&record.delta_borsh)
            .map_err(|e| StoreError::DataInconsistency(format!("palw v2 delta row for {block} does not decode: {e}")))?;
        Ok((record.state_root, delta))
    }

    // --- tip ---

    /// Stage the materialized tip: the state AT `block`, snapshotted. The recorded root is the
    /// state's own `state_root()`, computed here so a caller cannot store a snapshot under a
    /// root it does not hash to.
    pub fn set_tip_batch(&mut self, batch: &mut WriteBatch, block: BlockHash, state: &PalwChainStateV2) -> StoreResult<()> {
        let carriage = PalwStateCarriageV2::from_state(state);
        let carriage_borsh = borsh::to_vec(&carriage).expect("PalwStateCarriageV2 is borsh-serializable");
        self.tip.write(BatchDbWriter::new(batch), &PalwStateTipRecordV2 { block, state_root: state.state_root(), carriage_borsh })
    }

    /// Stage a tip row VERBATIM. Only the tests that must simulate post-write corruption use
    /// this — every production writer goes through [`Self::set_tip_batch`], which computes the
    /// root from the state it is handed, so a caller cannot store a snapshot under a root it
    /// does not hash to.
    #[cfg(test)]
    pub fn set_tip_record_batch(&mut self, batch: &mut WriteBatch, record: PalwStateTipRecordV2) -> StoreResult<()> {
        self.tip.write(BatchDbWriter::new(batch), &record)
    }

    /// The tip row undecoded (which block, which root), without rebuilding the state.
    pub fn tip_record(&self) -> StoreResult<Option<PalwStateTipRecordV2>> {
        match self.tip.read() {
            Ok(record) => Ok(Some(record)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Move the tip row to a block the sink does not stand at, reproducing what an unclean
    /// shutdown between this store's batch and the virtual-state commit leaves behind. Test-only:
    /// production writes the tip exactly where the UTXO walk ended.
    #[cfg(test)]
    pub fn set_tip_for_tests(&mut self, block: BlockHash, state: &PalwChainStateV2) -> StoreResult<()> {
        use kaspa_database::prelude::DirectDbWriter;
        let carriage = PalwStateCarriageV2::from_state(state);
        let carriage_borsh = borsh::to_vec(&carriage).expect("PalwStateCarriageV2 is borsh-serializable");
        self.tip.write(DirectDbWriter::new(&self.db), &PalwStateTipRecordV2 { block, state_root: state.state_root(), carriage_borsh })
    }

    /// Remove the tip row, reproducing the state a pruned join (or a `reindex_if_stale` after a
    /// schema bump) leaves behind: a live ConsensusV2 bundle with no PALW state under it. Test-only,
    /// because nothing in production should ever reach that state deliberately — the startup guard
    /// in `Consensus::new` refuses to run in it.
    #[cfg(test)]
    pub fn delete_tip_for_tests(&mut self) -> StoreResult<()> {
        use kaspa_database::prelude::DirectDbWriter;
        self.tip.remove(DirectDbWriter::new(&self.db))
    }

    /// Load and REBUILD the tip state, demanding the recorded root — index rebuild, internal
    /// consistency, deadline consistency and the root equality all run (`into_state`), so what
    /// this returns is a state the machine would have produced, or an error naming why not.
    /// Stage the pruning-point snapshot. Same shape and same derivation as [`Self::set_tip_batch`]
    /// — the root is computed from the state handed in, never carried.
    pub fn set_pruning_snapshot_batch(
        &mut self,
        batch: &mut WriteBatch,
        block: BlockHash,
        state: &PalwChainStateV2,
    ) -> StoreResult<()> {
        let carriage = PalwStateCarriageV2::from_state(state);
        let carriage_borsh = borsh::to_vec(&carriage).expect("PalwStateCarriageV2 is borsh-serializable");
        self.pruning_snapshot
            .write(BatchDbWriter::new(batch), &PalwStateTipRecordV2 { block, state_root: state.state_root(), carriage_borsh })
    }

    /// The raw snapshot row, or `None` when this node has not captured one yet — which is the
    /// honest answer for a node whose pruning point has not advanced since it started.
    pub fn pruning_snapshot_record(&self) -> StoreResult<Option<PalwStateTipRecordV2>> {
        match self.pruning_snapshot.read() {
            Ok(record) => Ok(Some(record)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The snapshot, decoded and re-checked against its own root, exactly as [`Self::load_tip`]
    /// does for the tip.
    pub fn load_pruning_snapshot(&self, params: &PalwStateParamsV2) -> StoreResult<Option<(BlockHash, PalwChainStateV2)>> {
        let Some(record) = self.pruning_snapshot_record()? else {
            return Ok(None);
        };
        let carriage = borsh::from_slice::<PalwStateCarriageV2>(&record.carriage_borsh)
            .map_err(|e| StoreError::DataInconsistency(format!("palw v2 pruning snapshot does not decode: {e}")))?;
        let state = carriage
            .into_state(params, Some(record.state_root))
            .map_err(|e: PalwStateV2Error| StoreError::DataInconsistency(format!("palw v2 pruning snapshot refused: {e}")))?;
        Ok(Some((record.block, state)))
    }

    pub fn load_tip(&self, params: &PalwStateParamsV2) -> StoreResult<Option<(BlockHash, PalwChainStateV2)>> {
        let Some(record) = self.tip_record()? else {
            return Ok(None);
        };
        let carriage = borsh::from_slice::<PalwStateCarriageV2>(&record.carriage_borsh)
            .map_err(|e| StoreError::DataInconsistency(format!("palw v2 tip snapshot does not decode: {e}")))?;
        let state = carriage
            .into_state(params, Some(record.state_root))
            .map_err(|e: PalwStateV2Error| StoreError::DataInconsistency(format!("palw v2 tip snapshot refused: {e}")))?;
        Ok(Some((record.block, state)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2, apply_palw_transition_v2, revert_delta_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap()
    }

    fn ctx(block_word: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: BlockHash::from_u64_word(block_word), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn registrations() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: Hash64::from_u64_word(1),
                artifact_root: Hash64::from_u64_word(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ]
    }

    /// One real transition, through the disk and back: the loaded tip is the state the machine
    /// produced (root-verified), and the loaded delta reverts it to its parent bit-for-bit —
    /// i.e. the reorg primitive survives serialization.
    #[test]
    fn a_transition_round_trips_through_disk_and_still_reverts() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let c1 = ctx(0xB1, 100, 100);
        let (child, delta) = apply_palw_transition_v2(&genesis, &p, &c1, &registrations(), None).unwrap();

        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, c1.block, child.state_root(), &delta).unwrap();
        store.set_tip_batch(&mut batch, c1.block, &child).unwrap();
        db.write(batch).unwrap();

        // A fresh store over the same database — i.e. a restart.
        let restarted = DbPalwStateV2Store::new(db, CachePolicy::Count(16));
        let (tip_block, tip_state) = restarted.load_tip(&p).unwrap().expect("the tip was written");
        assert_eq!(tip_block, c1.block);
        assert_eq!(tip_state, child, "the rebuilt tip is the state the machine produced, indices included");

        let (root, loaded_delta) = restarted.delta_of(c1.block).unwrap();
        assert_eq!(root, child.state_root());
        assert_eq!(loaded_delta, delta, "the delta's bytes are the machine's bytes");
        let reverted = revert_delta_v2(&tip_state, &loaded_delta, &p).unwrap();
        assert_eq!(reverted, genesis, "the reorg primitive survives the disk");
    }

    /// A tampered snapshot refuses to become a sink: the recorded root is demanded at load, so
    /// flipping one byte of the carriage is a named refusal, not a quietly different state.
    #[test]
    fn a_tampered_tip_snapshot_is_refused_not_loaded() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let c1 = ctx(0xB1, 100, 100);
        let (child, _delta) = apply_palw_transition_v2(&genesis, &p, &c1, &registrations(), None).unwrap();

        let mut batch = WriteBatch::default();
        store.set_tip_batch(&mut batch, c1.block, &child).unwrap();
        db.write(batch).unwrap();

        // Corrupt the snapshot bytes while keeping the record decodable: flip one byte of the
        // carriage body, write the record back verbatim otherwise.
        let mut record = store.tip_record().unwrap().unwrap();
        let last = record.carriage_borsh.len() - 1;
        record.carriage_borsh[last] ^= 0xFF;
        let mut tip_item: CachedDbItem<PalwStateTipRecordV2> =
            CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PalwStateV2Tip.into());
        tip_item.write(DirectDbWriter::new(&db), &record).unwrap();

        let restarted = DbPalwStateV2Store::new(db, CachePolicy::Count(16));
        match restarted.load_tip(&p) {
            Err(StoreError::DataInconsistency(msg)) => {
                assert!(msg.contains("refused") || msg.contains("decode"), "the refusal names the snapshot: {msg}")
            }
            other => panic!("a tampered snapshot must be refused, got {other:?}"),
        }
    }

    /// A schema bump discards rows and tip TOGETHER: a store that no longer understands its rows
    /// must read as empty-and-versioned, never as "no PALW work matured".
    #[test]
    fn a_schema_change_clears_rows_and_tip_together() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let c1 = ctx(0xB1, 100, 100);
        let (child, delta) = apply_palw_transition_v2(&genesis, &p, &c1, &registrations(), None).unwrap();
        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, c1.block, child.state_root(), &delta).unwrap();
        store.set_tip_batch(&mut batch, c1.block, &child).unwrap();
        db.write(batch).unwrap();

        // Simulate an old layout: stamp a different version, then reopen.
        let mut schema_item: CachedDbItem<u32> = CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PalwStateV2Schema.into());
        schema_item.write(DirectDbWriter::new(&db), &(PALW_STATE_V2_STORE_SCHEMA_VERSION - 1)).unwrap();

        let mut restarted = DbPalwStateV2Store::new(db, CachePolicy::Count(16));
        restarted.reindex_if_stale().unwrap();
        assert!(!restarted.has_delta(c1.block).unwrap(), "stale rows are discarded");
        assert!(restarted.tip_record().unwrap().is_none(), "and the tip goes with them — both or neither");
        assert_eq!(restarted.schema.read().unwrap(), PALW_STATE_V2_STORE_SCHEMA_VERSION);
    }
}
