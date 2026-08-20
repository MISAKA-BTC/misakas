//! MISAKA free-prompt PALW candidate-scoped state (ADR-0044 Unit C): the per-chain-block delta
//! rows a reorg walks, and the materialized anchor that keeps the walk short.
//!
//! # Why deltas and not states
//!
//! `PalwChainStateV2` is the whole registry — bonds, classes, claims, panels, courts. Persisting
//! one per chain block would store the world per block. What a reorg actually needs is the
//! **difference**: revert the deltas from the old sink down to the fork, apply the new branch's
//! up. `apply_delta_v2`/`revert_delta_v2` verify the value each entry replaces, so a delta
//! applied to the wrong parent is an error rather than a quiet divergence — which is what makes
//! a delta-based store unable to drift from the transition that produced it.
//!
//! The anchor is a materialized `PalwStateCarriageV2` plus the block it stands at, so a node
//! reconstructing a candidate's standing walks from there rather than from genesis.
//!
//! # Reorg discipline
//!
//! A delta row is written with the block that produced it, in the same `WriteBatch` as that
//! block's UTXO data, and it is **not** deleted when the block leaves the selected chain — a
//! reverted branch can be re-applied, and re-deriving its deltas would mean re-running the
//! transition for blocks the node already processed. Rows are pruned with their blocks
//! (the pruning traversal), exactly like `utxo_diffs`.
//!
//! # Nothing reads this yet on a shipped network
//!
//! Every preset is `PalwConsensusMode::Disabled` or `LegacyTn11`, and the writer is gated on the
//! mode carrying a `ConsensusV2` bundle. On every network that exists today this store stays
//! empty and no read path consults it.

use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw_state_v2::{PalwStateCarriageV2, PalwStateDeltaV2};
use kaspa_database::prelude::CachePolicy;
use kaspa_utils::mem_size::MemSizeEstimator;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// The layout these rows were written under.
///
/// Rows are written through the serde path, so a layout change makes existing rows undecodable —
/// and an undecodable delta reads as ABSENT, which is a reorg that silently does nothing. Bump
/// this on any change to `PalwStateDeltaV2` or `PalwStateCarriageV2` so stale rows are discarded
/// rather than misread.
pub const PALW_STATE_V2_STORE_SCHEMA_VERSION: u32 = 1;

/// One block's delta, as a row.
///
/// The row carries the delta's **canonical borsh bytes**, not a serde re-encoding of its fields.
/// A consensus object with two encodings is two objects that drift; the transition, the pruning
/// proof and this store all read the same bytes, and the wrapper exists only because the store
/// layer speaks serde.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PalwStateDeltaRow {
    pub delta_borsh: Vec<u8>,
}

impl PalwStateDeltaRow {
    pub fn encode(delta: &PalwStateDeltaV2) -> Self {
        Self { delta_borsh: borsh::to_vec(delta).expect("a delta is borsh-serializable") }
    }

    /// Decode, or say why not. A caller that swallowed this error would read a real delta as
    /// "this block changed nothing".
    pub fn decode(&self) -> Result<PalwStateDeltaV2, std::io::Error> {
        borsh::from_slice(&self.delta_borsh)
    }
}

impl MemSizeEstimator for PalwStateDeltaRow {
    fn estimate_mem_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.delta_borsh.len()
    }
}

/// The materialized point a delta walk starts from.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PalwStateAnchorRecord {
    /// The chain block whose state this carriage IS.
    pub block: BlockHash,
    /// The snapshot, in its canonical borsh form (same reason as the delta row above). Reloaded
    /// through `into_state(params, Some(root))` — never without the root, because a carriage's
    /// self-consistency cannot catch a coherent lie (its own doc says so).
    pub carriage_borsh: Vec<u8>,
    /// The state root the carriage must reproduce on load.
    pub state_root: kaspa_hashes::Hash64,
}

impl PalwStateAnchorRecord {
    pub fn encode(block: BlockHash, carriage: &PalwStateCarriageV2, state_root: kaspa_hashes::Hash64) -> Self {
        Self { block, carriage_borsh: borsh::to_vec(carriage).expect("a carriage is borsh-serializable"), state_root }
    }

    pub fn decode(&self) -> Result<PalwStateCarriageV2, std::io::Error> {
        borsh::from_slice(&self.carriage_borsh)
    }
}

/// The PALW state as-of the CURRENT pruning point, kept for serving a pruned peer (ADR-0042
/// Decision 5, ADR-0044 Unit E).
///
/// Why this is not the anchor: the anchor tracks the SINK and moves every virtual pass, so by the
/// time a peer asks, it stands far above the pruning point — and the peer has no blocks between
/// the two to walk down through. This row is the one PALW state that is meaningful to a node
/// which has deleted its history, so it is captured before pruning runs and moves only when the
/// pruning point does.
///
/// `state_root` is stored beside the snapshot for the same reason the anchor stores one: it is
/// what a receiver checks against the root a child header committed, and what this node checks on
/// its own reload.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct PalwPruningCarriageRecord {
    /// The pruning point whose state this carriage IS.
    pub pruning_point: BlockHash,
    pub carriage_borsh: Vec<u8>,
    pub state_root: kaspa_hashes::Hash64,
}

impl PalwPruningCarriageRecord {
    pub fn encode(pruning_point: BlockHash, carriage: &PalwStateCarriageV2, state_root: kaspa_hashes::Hash64) -> Self {
        Self { pruning_point, carriage_borsh: borsh::to_vec(carriage).expect("a carriage is borsh-serializable"), state_root }
    }

    pub fn decode(&self) -> Result<PalwStateCarriageV2, std::io::Error> {
        borsh::from_slice(&self.carriage_borsh)
    }
}

#[derive(Clone)]
pub struct DbPalwStateV2Store {
    db: Arc<DB>,
    deltas: CachedDbAccess<BlockHash, Arc<PalwStateDeltaRow>>,
    anchor: CachedDbItem<PalwStateAnchorRecord>,
    pruning_carriage: CachedDbItem<PalwPruningCarriageRecord>,
    schema: CachedDbItem<u32>,
}

impl DbPalwStateV2Store {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            deltas: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::PalwStateDeltas.into()),
            anchor: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::PalwStateAnchor.into()),
            pruning_carriage: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::PalwPruningCarriage.into()),
            schema: CachedDbItem::new(db, DatabaseStorePrefixes::PalwStateV2Schema.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Write one block's delta, in the caller's batch — the same batch as that block's UTXO data,
    /// so the two can never be half-written relative to each other.
    pub fn insert_delta_batch(&mut self, batch: &mut WriteBatch, block: BlockHash, delta: &PalwStateDeltaV2) -> StoreResult<()> {
        self.deltas.write(BatchDbWriter::new(batch), block, Arc::new(PalwStateDeltaRow::encode(delta)))
    }

    /// Drop one block's delta — the pruning traversal's call, never the reorg's (see the module
    /// doc: a reverted branch keeps its rows so re-applying it costs nothing).
    pub fn delete_delta_batch(&mut self, batch: &mut WriteBatch, block: BlockHash) -> StoreResult<()> {
        self.deltas.delete(BatchDbWriter::new(batch), block)
    }

    pub fn delta_row(&self, block: BlockHash) -> StoreResult<Arc<PalwStateDeltaRow>> {
        self.deltas.read(block)
    }

    pub fn has_delta(&self, block: BlockHash) -> StoreResult<bool> {
        self.deltas.has(block)
    }

    /// Move the materialized anchor. The caller supplies the root the carriage must reproduce;
    /// storing it beside the snapshot is what lets the loader verify rather than trust.
    pub fn set_anchor_batch(&mut self, batch: &mut WriteBatch, record: PalwStateAnchorRecord) -> StoreResult<()> {
        self.anchor.write(BatchDbWriter::new(batch), &record)
    }

    /// The anchor, or `None` when this node has never written one.
    pub fn anchor(&self) -> StoreResult<Option<PalwStateAnchorRecord>> {
        match self.anchor.read() {
            Ok(record) => Ok(Some(record)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Record the pruning point's carriage. Called BEFORE the pruning traversal deletes the
    /// delta rows it was derived from — after, the state is no longer reconstructible.
    pub fn set_pruning_carriage_batch(&mut self, batch: &mut WriteBatch, record: PalwPruningCarriageRecord) -> StoreResult<()> {
        self.pruning_carriage.write(BatchDbWriter::new(batch), &record)
    }

    /// The pruning point's carriage, or `None` when this node has never captured one (a node that
    /// has not pruned yet, or one on which V2 is dormant).
    pub fn pruning_carriage(&self) -> StoreResult<Option<PalwPruningCarriageRecord>> {
        match self.pruning_carriage.read() {
            Ok(record) => Ok(Some(record)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Discard rows written under a superseded layout. Called before any read: an undecodable
    /// delta reads as absent, and absent means "this block changed nothing", which is a
    /// live-looking answer that is simply wrong.
    pub fn reindex_if_stale(&mut self) -> Result<(), StoreError> {
        let stored = match self.schema.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(PALW_STATE_V2_STORE_SCHEMA_VERSION) {
            return Ok(());
        }
        if self.deltas.iterator().next().is_some() || self.anchor.read().is_ok() || self.pruning_carriage.read().is_ok() {
            kaspa_core::info!(
                "[palw-state-v2-store] rows were written under layout v{} and this build reads \
                 v{PALW_STATE_V2_STORE_SCHEMA_VERSION}; discarding them",
                stored.unwrap_or(0)
            );
            let mut batch = WriteBatch::default();
            self.deltas.delete_all(BatchDbWriter::new(&mut batch))?;
            self.anchor.remove(BatchDbWriter::new(&mut batch))?;
            // The pruning carriage goes too. It is the row a PEER would be handed, so a stale one
            // is not merely a slow local reload — it is this node serving a snapshot under a
            // layout the receiver does not read the same way.
            self.pruning_carriage.remove(BatchDbWriter::new(&mut batch))?;
            self.db.write(batch)?;
        }
        self.schema.write(kaspa_database::prelude::DirectDbWriter::new(&self.db), &PALW_STATE_V2_STORE_SCHEMA_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwClassDaaV2Params, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateParamsV2,
        apply_delta_v2, apply_palw_transition_v2, revert_delta_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn params() -> PalwStateParamsV2 {
        let class_daa = PalwClassDaaV2Params::new([(h64(1), 1000u16)].into_iter().collect(), 4).unwrap();
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, 800, class_daa).unwrap()
    }

    fn registrations() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(TransactionOutpoint {
                    transaction_id: TransactionId::from_u64_word(1),
                    index: 0,
                }),
                pubkey: vec![7; 4],
                operator_id: h64(21),
                collateral: 1_000,
            },
        ]
    }

    /// **The store cannot drift from the transition.** A delta written to disk, read back, and
    /// applied to its own parent reproduces the child the transition produced — and reverting it
    /// from that child reproduces the parent. This is the whole contract Unit C's reorg walk
    /// rests on, exercised through the real serde/rocksdb path rather than in memory.
    #[test]
    fn a_stored_delta_still_applies_and_reverts_exactly() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let p = params();
        let parent = PalwChainStateV2::genesis();
        let ctx = PalwBlockContextV2 { block: h64(0xB1), daa_score: 100, blue_score: 1 };
        let (child, delta) = apply_palw_transition_v2(&parent, &p, &ctx, &registrations(), None).unwrap();

        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, ctx.block, &delta).unwrap();
        db.write(batch).unwrap();

        let stored = store.delta_row(ctx.block).unwrap().decode().expect("a stored delta decodes");
        assert_eq!(stored, delta, "the row IS the delta — no re-encoding in between");
        assert_eq!(apply_delta_v2(&parent, &stored, &p).unwrap(), child, "replay from disk reproduces the transition");
        assert_eq!(revert_delta_v2(&child, &stored, &p).unwrap(), parent, "revert from disk reproduces the parent");

        assert!(store.has_delta(ctx.block).unwrap());
        assert!(store.delta_row(h64(0xDEAD)).is_err(), "an absent block has no delta — never a silent empty one");
    }

    /// The anchor round-trips under its committed root, and a tampered snapshot cannot load —
    /// the carriage's own rule (self-consistency cannot catch a coherent lie), enforced where the
    /// snapshot comes off disk.
    #[test]
    fn the_anchor_round_trips_and_refuses_a_tampered_snapshot() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();
        assert!(store.anchor().unwrap().is_none(), "a fresh node has no anchor");

        let p = params();
        let ctx = PalwBlockContextV2 { block: h64(0xB1), daa_score: 100, blue_score: 1 };
        let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx, &registrations(), None).unwrap();
        let root = state.state_root();
        let carriage = PalwStateCarriageV2::from_state(&state);

        let mut batch = WriteBatch::default();
        store.set_anchor_batch(&mut batch, PalwStateAnchorRecord::encode(ctx.block, &carriage, root)).unwrap();
        db.write(batch).unwrap();

        let record = store.anchor().unwrap().expect("the anchor is there");
        assert_eq!(record.block, ctx.block);
        assert_eq!(record.state_root, root);
        let reloaded = record.decode().unwrap().into_state(&p, Some(root)).expect("the honest anchor loads under its root");
        assert_eq!(reloaded, state);

        // A snapshot that no longer hashes to its committed root must not load, even though it is
        // internally coherent.
        let mut tampered = record.decode().unwrap();
        tampered.safe_weight += 1;
        let record = PalwStateAnchorRecord::encode(ctx.block, &tampered, root);
        assert!(record.decode().unwrap().into_state(&p, Some(root)).is_err(), "a tampered anchor cannot load under the real root");
    }

    /// A layout bump discards rows rather than letting them read as absent — an undecodable delta
    /// would otherwise be a reorg that silently does nothing.
    #[test]
    fn a_layout_bump_discards_rows() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let p = params();
        let ctx = PalwBlockContextV2 { block: h64(0xB1), daa_score: 100, blue_score: 1 };
        let (_, delta) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx, &registrations(), None).unwrap();
        let mut batch = WriteBatch::default();
        store.insert_delta_batch(&mut batch, ctx.block, &delta).unwrap();
        db.write(batch).unwrap();
        assert!(store.has_delta(ctx.block).unwrap());

        // Simulate a build whose layout marker differs: write a stale version, then re-run the
        // gate the way a node does at startup.
        let mut stale = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        stale.schema.write(kaspa_database::prelude::DirectDbWriter::new(&db), &(PALW_STATE_V2_STORE_SCHEMA_VERSION + 1)).unwrap();
        stale.reindex_if_stale().unwrap();
        assert!(!stale.has_delta(ctx.block).unwrap(), "rows from a superseded layout are gone, not misread");
        assert!(stale.anchor().unwrap().is_none());
    }
}
