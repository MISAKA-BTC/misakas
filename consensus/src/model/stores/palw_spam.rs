//! Fork-local PALW Header-v4 anti-spam accumulator.
//!
//! Each immutable block row stores cumulative blue-lane counters, its selected parent, and one
//! deterministic bounded rolling/checkpoint pointer. Within a power-of-two checkpoint the pointer is
//! the Fenwick low-bit predecessor; checkpoint rows point to the immediately preceding checkpoint.
//! The row remains fixed-size, lookup is logarithmic, and no valid pointer can escape the finite
//! pruning horizon derived from the configured DAA window.

use std::{
    collections::{BinaryHeap, HashMap, HashSet},
    mem::size_of,
    sync::Arc,
};

use kaspa_consensus_core::{BlockHash, BlockHasher, palw_antispam::palw_spam_checkpoint_span};
use kaspa_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DirectDbWriter, StoreError},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

/// Consensus fail-closed bound for any skip/parent lookup. The deterministic skip construction needs
/// O(log2(u64::MAX)) hops; 4× the bit width leaves a conservative proof/debug margin.
pub const PALW_SPAM_MAX_LOOKUP_HOPS: usize = 4 * u64::BITS as usize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwSpamAccumulatorV1 {
    pub version: u16,
    pub daa_score: u64,
    /// Height in this row's selected-parent chain; the re-genesis root is height zero.
    pub selected_height: u64,
    /// Cumulative blue hash-lane admission events in this immutable fork history.
    pub total_hash_blues: u64,
    /// Cumulative blue replica-lane admission events in this immutable fork history.
    pub total_replica_blues: u64,
    pub selected_parent: Option<BlockHash>,
    /// Ancestor at [`palw_spam_skip_height`] for `selected_height` and the network window, present
    /// iff height >= 2.
    pub skip: Option<BlockHash>,
}

impl PalwSpamAccumulatorV1 {
    pub fn root(daa_score: u64) -> Self {
        Self {
            version: 1,
            daa_score,
            selected_height: 0,
            total_hash_blues: 0,
            total_replica_blues: 0,
            selected_parent: None,
            skip: None,
        }
    }

    pub fn validate_shape(&self) -> Result<(), PalwSpamAccumulatorError> {
        let pointers_valid = match self.selected_height {
            0 => self.selected_parent.is_none() && self.skip.is_none() && self.total_hash_blues == 0 && self.total_replica_blues == 0,
            1 => self.selected_parent.is_some() && self.skip.is_none(),
            _ => self.selected_parent.is_some() && self.skip.is_some(),
        };
        if self.version != 1 || !pointers_valid {
            return Err(PalwSpamAccumulatorError::MalformedState);
        }
        Ok(())
    }

    pub fn commitment(&self) -> BlockHash {
        kaspa_consensus_core::palw_antispam::palw_spam_accumulator_commitment(
            self.daa_score,
            self.selected_height,
            self.total_hash_blues,
            self.total_replica_blues,
            self.selected_parent,
            self.skip,
        )
    }
}

impl Default for PalwSpamAccumulatorV1 {
    fn default() -> Self {
        Self::root(0)
    }
}

impl MemSizeEstimator for PalwSpamAccumulatorV1 {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwSpamLaneDelta {
    pub hash_blues: u64,
    pub replica_blues: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwSpamAccumulatorError {
    #[error("missing PALW spam accumulator for active block {0}")]
    Missing(BlockHash),
    #[error("malformed PALW spam accumulator or non-deterministic skip pointer")]
    MalformedState,
    #[error("PALW spam accumulator lookup exceeded {PALW_SPAM_MAX_LOOKUP_HOPS} hops")]
    LookupBoundExceeded,
    #[error("PALW spam accumulator arithmetic overflow")]
    Overflow,
    #[error("PALW spam accumulator totals or DAA score moved backwards")]
    NonMonotonic,
    #[error("PALW spam accumulator store error: {0}")]
    Store(String),
    #[error("PALW spam accumulator window has no bounded checkpoint span")]
    InvalidWindow,
}

pub trait PalwSpamAccumulatorStoreReader {
    fn get_optional(&self, hash: BlockHash) -> Result<Option<Arc<PalwSpamAccumulatorV1>>, StoreError>;
}

#[derive(Clone)]
pub struct DbPalwSpamAccumulatorStore {
    db: Arc<kaspa_database::prelude::DB>,
    access: CachedDbAccess<BlockHash, Arc<PalwSpamAccumulatorV1>, BlockHasher>,
}

impl DbPalwSpamAccumulatorStore {
    pub fn new(db: Arc<kaspa_database::prelude::DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwSpamAccumulator.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, state: Arc<PalwSpamAccumulatorV1>) -> Result<(), StoreError> {
        state.validate_shape().map_err(|e| StoreError::DataInconsistency(e.to_string()))?;
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, state)
    }

    pub fn insert(&self, hash: BlockHash, state: Arc<PalwSpamAccumulatorV1>) -> Result<(), StoreError> {
        state.validate_shape().map_err(|e| StoreError::DataInconsistency(e.to_string()))?;
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, state)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<(BlockHash, Arc<PalwSpamAccumulatorV1>), StoreError>> + '_ {
        self.access.iterator().map(|row| match row {
            Ok((key, state)) => <[u8; 64]>::try_from(key.as_ref())
                .map(|bytes| (BlockHash::from_bytes(bytes), state))
                .map_err(|_| StoreError::DataInconsistency("PALW spam accumulator key is not 64 bytes".into())),
            Err(err) => Err(StoreError::DataInconsistency(format!("PALW spam accumulator iterator: {err}"))),
        })
    }
}

impl PalwSpamAccumulatorStoreReader for DbPalwSpamAccumulatorStore {
    fn get_optional(&self, hash: BlockHash) -> Result<Option<Arc<PalwSpamAccumulatorV1>>, StoreError> {
        use kaspa_database::prelude::StoreResultExt;
        self.access.read(hash).optional()
    }
}

fn read_required<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    hash: BlockHash,
) -> Result<Arc<PalwSpamAccumulatorV1>, PalwSpamAccumulatorError> {
    let state = store
        .get_optional(hash)
        .map_err(|e| PalwSpamAccumulatorError::Store(e.to_string()))?
        .ok_or(PalwSpamAccumulatorError::Missing(hash))?;
    state.validate_shape()?;
    Ok(state)
}

/// Deterministic bounded rolling/checkpoint skip height. Non-checkpoint rows clear their Fenwick low
/// bit and stay inside the current checkpoint; checkpoint rows advance exactly one checkpoint. The
/// maximum pointer distance is therefore the power-of-two span derived from `window_daa`.
pub const fn palw_spam_skip_height(height: u64, window_daa: u64) -> u64 {
    let span = palw_spam_checkpoint_span(window_daa);
    if height < 2 || span == 0 {
        0
    } else {
        let low_bit = height & height.wrapping_neg();
        height - if low_bit < span { low_bit } else { span }
    }
}

/// Validate DAA ordering over one authenticated parent or skip link. Consensus excludes the
/// fixed-timestamp genesis from the DAA window, so the re-genesis root and its direct children share
/// one score. That root (height 0) -> child (height 1) edge is the only equality allowed; every later
/// direct edge and every multi-height skip must be strict.
fn palw_spam_daa_link_is_valid(ancestor: &PalwSpamAccumulatorV1, child_height: u64, child_daa_score: u64) -> bool {
    child_height > ancestor.selected_height
        && (child_daa_score > ancestor.daa_score
            || (ancestor.selected_height == 0 && child_height == 1 && child_daa_score == ancestor.daa_score))
}

fn checked_link<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    cursor: &PalwSpamAccumulatorV1,
    hash: BlockHash,
    expected_height: u64,
) -> Result<Arc<PalwSpamAccumulatorV1>, PalwSpamAccumulatorError> {
    let next = read_required(store, hash)?;
    if next.selected_height != expected_height
        || next.selected_height >= cursor.selected_height
        || !palw_spam_daa_link_is_valid(&next, cursor.selected_height, cursor.daa_score)
        || next.total_hash_blues > cursor.total_hash_blues
        || next.total_replica_blues > cursor.total_replica_blues
    {
        return Err(PalwSpamAccumulatorError::MalformedState);
    }
    Ok(next)
}

fn selected_ancestor_at_height<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    start_hash: BlockHash,
    start: &PalwSpamAccumulatorV1,
    target_height: u64,
    window_daa: u64,
) -> Result<BlockHash, PalwSpamAccumulatorError> {
    if target_height > start.selected_height || palw_spam_checkpoint_span(window_daa) == 0 {
        return Err(PalwSpamAccumulatorError::MalformedState);
    }
    let mut cursor_hash = start_hash;
    let mut cursor = Arc::new(start.clone());
    let mut hops = 0usize;
    while cursor.selected_height > target_height {
        if hops >= PALW_SPAM_MAX_LOOKUP_HOPS {
            return Err(PalwSpamAccumulatorError::LookupBoundExceeded);
        }
        hops += 1;
        let skip_height = palw_spam_skip_height(cursor.selected_height, window_daa);
        let use_skip = cursor.skip.is_some() && skip_height >= target_height;
        if use_skip {
            let hash = cursor.skip.ok_or(PalwSpamAccumulatorError::MalformedState)?;
            cursor = checked_link(store, &cursor, hash, skip_height)?;
            cursor_hash = hash;
        } else {
            let hash = cursor.selected_parent.ok_or(PalwSpamAccumulatorError::MalformedState)?;
            cursor = checked_link(store, &cursor, hash, cursor.selected_height - 1)?;
            cursor_hash = hash;
        }
    }
    Ok(cursor_hash)
}

fn build_child_skip<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    selected_parent: BlockHash,
    parent: &PalwSpamAccumulatorV1,
    child_height: u64,
    window_daa: u64,
) -> Result<Option<BlockHash>, PalwSpamAccumulatorError> {
    if child_height < 2 {
        return Ok(None);
    }
    let target = palw_spam_skip_height(child_height, window_daa);
    if target == parent.selected_height {
        Ok(Some(selected_parent))
    } else {
        selected_ancestor_at_height(store, selected_parent, parent, target, window_daa).map(Some)
    }
}

/// Find the newest selected-chain transition whose DAA score is `<= lower_inclusive`.
///
/// Skip links are taken only while their authenticated row remains strictly above the boundary; the
/// final parent step therefore cannot overshoot. Lookup is capped by [`PALW_SPAM_MAX_LOOKUP_HOPS`].
pub fn palw_spam_window_baseline<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    selected_parent: BlockHash,
    lower_inclusive: u64,
    window_daa: u64,
) -> Result<Option<Arc<PalwSpamAccumulatorV1>>, PalwSpamAccumulatorError> {
    if palw_spam_checkpoint_span(window_daa) == 0 {
        return Err(PalwSpamAccumulatorError::InvalidWindow);
    }
    let mut cursor = read_required(store, selected_parent)?;
    let mut hops = 0usize;
    while cursor.daa_score > lower_inclusive {
        if hops >= PALW_SPAM_MAX_LOOKUP_HOPS {
            return Err(PalwSpamAccumulatorError::LookupBoundExceeded);
        }
        hops += 1;
        if let Some(skip_hash) = cursor.skip {
            let skip_height = palw_spam_skip_height(cursor.selected_height, window_daa);
            let height_delta = cursor.selected_height - skip_height;
            let daa_above_lower = cursor.daa_score - lower_inclusive;
            // Selected-chain DAA growth is strict after the sole root->height-one equality edge.
            // That edge is relevant only while the retained path still contains the root. A target
            // at least `daa_above_lower` heights back is therefore already at/below the inclusive
            // boundary; avoid reading that deliberately prunable cross-checkpoint row.
            if height_delta < daa_above_lower {
                let skip = checked_link(store, &cursor, skip_hash, skip_height)?;
                if skip.daa_score > lower_inclusive {
                    cursor = skip;
                    continue;
                }
            }
        }
        let Some(parent_hash) = cursor.selected_parent else { return Ok(None) };
        cursor = checked_link(store, &cursor, parent_hash, cursor.selected_height - 1)?;
    }
    Ok(Some(cursor))
}

/// Exact selected-parent rows which make both a window baseline and every future child skip pointer
/// self-contained at a retained boundary. The path is newest-to-oldest and includes `tip`. Since the
/// checkpoint span is at least the DAA window and DAA grows strictly after the sole re-genesis edge,
/// one full checkpoint behind the tip is a complete finite closure (early paths include the root).
pub fn palw_spam_retained_path<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    tip: BlockHash,
    window_daa: u64,
) -> Result<Vec<(BlockHash, Arc<PalwSpamAccumulatorV1>)>, PalwSpamAccumulatorError> {
    let span = palw_spam_checkpoint_span(window_daa);
    if span == 0 {
        return Err(PalwSpamAccumulatorError::InvalidWindow);
    }
    let mut hash = tip;
    let mut state = read_required(store, hash)?;
    let floor = state.selected_height.saturating_sub(span);
    let mut path = Vec::with_capacity((state.selected_height - floor + 1) as usize);
    loop {
        if state.selected_height >= 2 {
            let skip_height = palw_spam_skip_height(state.selected_height, window_daa);
            if skip_height >= floor {
                let skip_hash = state.skip.ok_or(PalwSpamAccumulatorError::MalformedState)?;
                checked_link(store, &state, skip_hash, skip_height)?;
            }
        }
        path.push((hash, state.clone()));
        if state.selected_height == floor {
            break;
        }
        let parent_hash = state.selected_parent.ok_or(PalwSpamAccumulatorError::MalformedState)?;
        state = checked_link(store, &state, parent_hash, state.selected_height - 1)?;
        hash = parent_hash;
    }
    Ok(path)
}

pub fn palw_spam_retained_closure<S: PalwSpamAccumulatorStoreReader + ?Sized, I: IntoIterator<Item = BlockHash>>(
    store: &S,
    tips: I,
    window_daa: u64,
) -> Result<HashSet<BlockHash>, PalwSpamAccumulatorError> {
    let span = palw_spam_checkpoint_span(window_daa);
    if span == 0 {
        return Err(PalwSpamAccumulatorError::InvalidWindow);
    }

    // Process the largest remaining parent budget first. If many retained headers share a selected
    // past, each row is expanded only for the maximum budget reaching it instead of re-walking a
    // complete checkpoint per seed. This marks exactly the union of `retained_path(tip)` results.
    let mut work = BinaryHeap::new();
    for tip in tips {
        work.push((span, tip));
    }
    let mut coverage = HashMap::<BlockHash, u64>::new();
    let mut states = HashMap::<BlockHash, Arc<PalwSpamAccumulatorV1>>::new();
    while let Some((requested_budget, hash)) = work.pop() {
        let state = match states.get(&hash) {
            Some(state) => state.clone(),
            None => {
                let state = read_required(store, hash)?;
                states.insert(hash, state.clone());
                state
            }
        };
        let budget = requested_budget.min(state.selected_height);
        if coverage.get(&hash).is_some_and(|covered| *covered >= budget) {
            continue;
        }
        coverage.insert(hash, budget);
        if budget > 0 {
            let parent_hash = state.selected_parent.ok_or(PalwSpamAccumulatorError::MalformedState)?;
            let parent = checked_link(store, &state, parent_hash, state.selected_height - 1)?;
            states.entry(parent_hash).or_insert(parent);
            work.push((budget - 1, parent_hash));
        }
    }

    // Validate every deterministic skip which lies inside at least one requested closure. A floor
    // row's older skip is intentionally not read: no retained child can use it.
    for (hash, budget) in &coverage {
        let state = states.get(hash).ok_or(PalwSpamAccumulatorError::MalformedState)?;
        if state.selected_height >= 2 {
            let skip_height = palw_spam_skip_height(state.selected_height, window_daa);
            if state.selected_height - skip_height <= *budget {
                let skip_hash = state.skip.ok_or(PalwSpamAccumulatorError::MalformedState)?;
                checked_link(store, state, skip_hash, skip_height)?;
                if !coverage.contains_key(&skip_hash) {
                    return Err(PalwSpamAccumulatorError::MalformedState);
                }
            }
        }
    }
    Ok(coverage.into_keys().collect())
}

/// Fully preflight a reclaim sweep and return only rows outside every retained closure. Callers must
/// not stage any deletes until this function succeeds: a missing ancestor or corrupt pointer is a
/// fail-closed result which preserves the complete store for repair/restart.
pub fn palw_spam_reclaim_candidates<
    S: PalwSpamAccumulatorStoreReader + ?Sized,
    R: IntoIterator<Item = BlockHash>,
    T: IntoIterator<Item = BlockHash>,
    P: IntoIterator<Item = BlockHash>,
>(
    store: &S,
    all_rows: R,
    retained_tips: T,
    pinned_rows: P,
    window_daa: u64,
) -> Result<Vec<BlockHash>, PalwSpamAccumulatorError> {
    let all_rows: Vec<BlockHash> = all_rows.into_iter().collect();
    let all_row_set: HashSet<BlockHash> = all_rows.iter().copied().collect();
    let mut retained = palw_spam_retained_closure(store, retained_tips, window_daa)?;
    for hash in pinned_rows {
        if !all_row_set.contains(&hash) {
            return Err(PalwSpamAccumulatorError::Missing(hash));
        }
        read_required(store, hash)?;
        retained.insert(hash);
    }
    let mut reclaim: Vec<BlockHash> = all_rows.into_iter().filter(|hash| !retained.contains(hash)).collect();
    reclaim.sort_unstable_by_key(|hash| hash.as_bytes());
    Ok(reclaim)
}

/// Derive a child row and the exact counts in its selected-chain admission-event horizon.
///
/// `new_blue_mergeset` excludes the selected parent and candidate. Its sources enter the accumulator
/// at this child transition, which prevents sampled placement from hiding them. The returned counts
/// exclude the candidate; `palw_spam_target` adds a prospective replica exactly once.
pub fn palw_spam_derive_child<S: PalwSpamAccumulatorStoreReader + ?Sized>(
    store: &S,
    selected_parent: BlockHash,
    child_daa_score: u64,
    window_daa: u64,
    new_blue_mergeset: PalwSpamLaneDelta,
    child_is_replica: bool,
) -> Result<(PalwSpamAccumulatorV1, kaspa_consensus_core::palw_antispam::PalwSpamWindowCounts), PalwSpamAccumulatorError> {
    if palw_spam_checkpoint_span(window_daa) == 0 {
        return Err(PalwSpamAccumulatorError::InvalidWindow);
    }
    let parent = read_required(store, selected_parent)?;
    let selected_height = parent.selected_height.checked_add(1).ok_or(PalwSpamAccumulatorError::Overflow)?;
    // Genesis is excluded from the DAA window, so every direct child shares its DAA score. Permit
    // exactly that re-genesis boundary edge; strict growth applies to all subsequent transitions and
    // turns the configured DAA window into the selected-height retention bound proven above.
    if !palw_spam_daa_link_is_valid(&parent, selected_height, child_daa_score) {
        return Err(PalwSpamAccumulatorError::NonMonotonic);
    }
    let past_hash = parent.total_hash_blues.checked_add(new_blue_mergeset.hash_blues).ok_or(PalwSpamAccumulatorError::Overflow)?;
    let past_replica =
        parent.total_replica_blues.checked_add(new_blue_mergeset.replica_blues).ok_or(PalwSpamAccumulatorError::Overflow)?;

    let lower = child_daa_score.saturating_sub(window_daa);
    let baseline = palw_spam_window_baseline(store, selected_parent, lower, window_daa)?;
    let baseline_hash = baseline.as_ref().map_or(0, |s| s.total_hash_blues);
    let baseline_replica = baseline.as_ref().map_or(0, |s| s.total_replica_blues);
    let counts = kaspa_consensus_core::palw_antispam::PalwSpamWindowCounts {
        hash_blues: past_hash.checked_sub(baseline_hash).ok_or(PalwSpamAccumulatorError::NonMonotonic)?,
        replica_blues: past_replica.checked_sub(baseline_replica).ok_or(PalwSpamAccumulatorError::NonMonotonic)?,
    };

    let total_hash_blues = past_hash.checked_add(u64::from(!child_is_replica)).ok_or(PalwSpamAccumulatorError::Overflow)?;
    let total_replica_blues = past_replica.checked_add(u64::from(child_is_replica)).ok_or(PalwSpamAccumulatorError::Overflow)?;
    let skip = build_child_skip(store, selected_parent, &parent, selected_height, window_daa)?;
    let state = PalwSpamAccumulatorV1 {
        version: 1,
        daa_score: child_daa_score,
        selected_height,
        total_hash_blues,
        total_replica_blues,
        selected_parent: Some(selected_parent),
        skip,
    };
    state.validate_shape()?;
    Ok((state, counts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{
        create_temp_db,
        prelude::{ConnBuilder, StoreError},
    };
    use kaspa_hashes::Hash64;
    use std::{cell::Cell, collections::HashMap};

    #[test]
    fn default_accumulator_is_a_valid_v1_root() {
        let state = PalwSpamAccumulatorV1::default();
        assert_eq!(state, PalwSpamAccumulatorV1::root(0));
        state.validate_shape().unwrap();
    }

    #[derive(Default)]
    struct MemoryStore {
        rows: HashMap<BlockHash, Arc<PalwSpamAccumulatorV1>>,
        reads: Cell<usize>,
    }
    impl PalwSpamAccumulatorStoreReader for MemoryStore {
        fn get_optional(&self, hash: BlockHash) -> Result<Option<Arc<PalwSpamAccumulatorV1>>, StoreError> {
            self.reads.set(self.reads.get() + 1);
            Ok(self.rows.get(&hash).cloned())
        }
    }
    impl MemoryStore {
        fn put(&mut self, hash: BlockHash, state: PalwSpamAccumulatorV1) {
            self.rows.insert(hash, Arc::new(state));
        }
        fn reset_reads(&self) {
            self.reads.set(0);
        }
    }
    fn h(n: u64) -> BlockHash {
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(&n.to_le_bytes());
        Hash64::from_bytes(bytes)
    }

    fn branch_h(branch: u8, height: u64) -> BlockHash {
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(&height.to_le_bytes());
        bytes[63] = branch;
        Hash64::from_bytes(bytes)
    }

    fn delta(height: u64) -> PalwSpamLaneDelta {
        PalwSpamLaneDelta { hash_blues: u64::from(height.is_multiple_of(11)), replica_blues: u64::from(height.is_multiple_of(13)) }
    }
    fn child(
        store: &mut MemoryStore,
        hash: BlockHash,
        parent: BlockHash,
        daa: u64,
        window: u64,
        delta: PalwSpamLaneDelta,
        replica: bool,
    ) -> kaspa_consensus_core::palw_antispam::PalwSpamWindowCounts {
        let (state, counts) = palw_spam_derive_child(store, parent, daa, window, delta, replica).unwrap();
        store.put(hash, state);
        counts
    }

    #[test]
    fn siblings_are_fork_local_and_merge_counts_both_without_sampling() {
        let mut s = MemoryStore::default();
        s.put(h(0), PalwSpamAccumulatorV1::root(0));
        child(&mut s, h(1), h(0), 1, 100, PalwSpamLaneDelta::default(), false);
        let left = child(&mut s, h(2), h(1), 2, 100, PalwSpamLaneDelta::default(), true);
        let right = child(&mut s, h(3), h(1), 2, 100, PalwSpamLaneDelta::default(), true);
        assert_eq!(left, right);
        let merged = child(&mut s, h(4), h(2), 3, 100, PalwSpamLaneDelta { hash_blues: 0, replica_blues: 1 }, false);
        assert_eq!((merged.hash_blues, merged.replica_blues), (1, 2));
        assert_eq!((s.rows[&h(4)].total_hash_blues, s.rows[&h(4)].total_replica_blues), (2, 2));
    }

    #[test]
    fn competing_reorg_branches_never_leak_counters_or_skip_links() {
        let mut s = MemoryStore::default();
        s.put(h(0), PalwSpamAccumulatorV1::root(0));
        child(&mut s, h(1), h(0), 1, 100, PalwSpamLaneDelta::default(), false);
        child(&mut s, h(2), h(1), 2, 100, PalwSpamLaneDelta::default(), true);
        child(&mut s, h(3), h(1), 2, 100, PalwSpamLaneDelta::default(), false);
        child(&mut s, h(4), h(2), 3, 100, PalwSpamLaneDelta::default(), true);
        child(&mut s, h(5), h(3), 3, 100, PalwSpamLaneDelta::default(), false);
        assert_eq!((s.rows[&h(4)].total_hash_blues, s.rows[&h(4)].total_replica_blues), (1, 2));
        assert_eq!((s.rows[&h(5)].total_hash_blues, s.rows[&h(5)].total_replica_blues), (3, 0));
        assert_ne!(s.rows[&h(4)].selected_parent, s.rows[&h(5)].selected_parent);
    }

    #[test]
    fn bounded_checkpoint_skip_handles_checkpoint_and_power_of_two_edges() {
        const WINDOW: u64 = 13;
        const SPAN: u64 = 16;
        assert_eq!(palw_spam_checkpoint_span(WINDOW), SPAN);
        assert_eq!(palw_spam_skip_height(15, WINDOW), 14);
        assert_eq!(palw_spam_skip_height(16, WINDOW), 0);
        assert_eq!(palw_spam_skip_height(17, WINDOW), 16);
        assert_eq!(palw_spam_skip_height(31, WINDOW), 30);
        assert_eq!(palw_spam_skip_height(32, WINDOW), 16);
        assert_eq!(palw_spam_skip_height(48, WINDOW), 32);
        assert_eq!(palw_spam_skip_height(1 << 20, WINDOW), (1 << 20) - SPAN);
        assert_eq!(palw_spam_skip_height(1 << 63, WINDOW), (1 << 63) - SPAN);

        for height in 2..=100_000 {
            let target = palw_spam_skip_height(height, WINDOW);
            assert!(target < height);
            assert!(height - target <= SPAN, "height {height} escaped its checkpoint horizon");
        }
    }

    #[test]
    fn active_v4_accumulator_allows_only_the_genesis_daa_equality_edge() {
        let mut store = MemoryStore::default();
        store.put(h(0), PalwSpamAccumulatorV1::root(10));
        assert_eq!(
            palw_spam_derive_child(&store, h(0), 9, 17, PalwSpamLaneDelta::default(), false),
            Err(PalwSpamAccumulatorError::NonMonotonic)
        );

        let (first, _) = palw_spam_derive_child(&store, h(0), 10, 17, PalwSpamLaneDelta::default(), false).unwrap();
        store.put(h(1), first);
        assert_eq!(
            palw_spam_derive_child(&store, h(1), 10, 17, PalwSpamLaneDelta::default(), false),
            Err(PalwSpamAccumulatorError::NonMonotonic)
        );
        // Height two builds a skip to height zero, exercising the authenticated equal-score root link.
        let (second, _) = palw_spam_derive_child(&store, h(1), 11, 17, PalwSpamLaneDelta::default(), false).unwrap();
        store.put(h(2), second);
        assert_eq!(palw_spam_window_baseline(&store, h(2), 10, 17).unwrap().unwrap().selected_height, 1);
        assert_eq!(palw_spam_retained_path(&store, h(2), 17).unwrap().len(), 3);
        assert_eq!(palw_spam_retained_closure(&store, [h(2)], 17).unwrap(), HashSet::from([h(0), h(1), h(2)]));

        // A stored non-root equality must also fail closed when reached through snapshot/retention
        // traversal, rather than relying only on construction-time validation.
        let mut forged = (*store.rows[&h(2)]).clone();
        forged.daa_score = 10;
        store.put(h(3), forged);
        assert!(matches!(palw_spam_retained_path(&store, h(3), 17), Err(PalwSpamAccumulatorError::MalformedState)));
        assert!(palw_spam_derive_child(&store, h(0), 11, 17, PalwSpamLaneDelta::default(), false).is_ok());
    }

    #[test]
    fn exact_window_boundary_and_skip_pointer_are_rederived() {
        let mut s = MemoryStore::default();
        s.put(h(0), PalwSpamAccumulatorV1::root(0));
        for daa in 1..=40 {
            let counts = child(&mut s, h(daa), h(daa - 1), daa, 3, PalwSpamLaneDelta::default(), daa % 2 == 0);
            if daa == 40 {
                assert_eq!((counts.hash_blues, counts.replica_blues), (1, 1));
            }
            let row = &s.rows[&h(daa)];
            if daa >= 2 {
                let skip = row.skip.expect("height >= 2 has a deterministic skip");
                assert_eq!(s.rows[&skip].selected_height, palw_spam_skip_height(daa, 3));
            }
        }

        let mut forged = (*s.rows[&h(40)]).clone();
        forged.skip = Some(h(39));
        s.put(h(41), forged);
        assert_eq!(
            palw_spam_window_baseline(&s, h(41), 10, 3),
            Err(PalwSpamAccumulatorError::MalformedState),
            "a committed but non-deterministic shortcut is never trusted"
        );
    }

    #[test]
    fn large_horizon_lookup_is_monotonic_and_bounded() {
        const TIP: u64 = 60_000;
        const WINDOW: u64 = 26_440;
        let mut s = MemoryStore::default();
        s.put(h(0), PalwSpamAccumulatorV1::root(0));
        for daa in 1..=TIP {
            child(&mut s, h(daa), h(daa - 1), daa, WINDOW, PalwSpamLaneDelta::default(), daa % 5 == 0);
        }
        for lower in [0, 1, 17, TIP - WINDOW, TIP - 1, TIP] {
            s.reset_reads();
            let baseline = palw_spam_window_baseline(&s, h(TIP), lower, WINDOW).unwrap().unwrap();
            assert_eq!(baseline.daa_score, lower);
            assert!(s.reads.get() <= PALW_SPAM_MAX_LOOKUP_HOPS + 1, "{} reads at lower {lower}", s.reads.get());
        }
    }

    #[test]
    fn skip_lookup_matches_linear_oracle_across_variable_daa_history() {
        const TIP: u64 = 40_000;
        const WINDOW: u64 = 26_440;
        let mut s = MemoryStore::default();
        let mut daa_by_height = Vec::with_capacity(TIP as usize + 1);
        daa_by_height.push(0u64);
        s.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=TIP {
            // Deterministic non-uniform DAA progression exercises boundaries that do not coincide
            // with selected height. The real DAA increment is positive but can exceed one when a
            // transition admits several DAA blocks.
            let daa = daa_by_height[(height - 1) as usize] + 1 + u64::from(height.is_multiple_of(17)) * 3;
            daa_by_height.push(daa);
            child(&mut s, h(height), h(height - 1), daa, WINDOW, PalwSpamLaneDelta::default(), height.is_multiple_of(7));
        }

        // Xorshift gives a reproducible property sweep without adding a test dependency.
        let mut random = 0x9e37_79b9_7f4a_7c15u64;
        for _ in 0..2_048 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let tip_height = 1 + random % TIP;
            let tip_daa = daa_by_height[tip_height as usize];
            let lower = random.rotate_left(23) % (tip_daa + 1);
            let expected_height = daa_by_height[..=tip_height as usize].partition_point(|daa| *daa <= lower).saturating_sub(1) as u64;

            s.reset_reads();
            let actual = palw_spam_window_baseline(&s, h(tip_height), lower, WINDOW).unwrap().unwrap();
            assert_eq!(actual.selected_height, expected_height, "tip={tip_height}, lower={lower}");
            assert!(s.reads.get() <= PALW_SPAM_MAX_LOOKUP_HOPS + 1, "{} reads at tip {tip_height}, lower {lower}", s.reads.get());
        }
    }

    #[test]
    fn retained_checkpoint_property_matches_from_genesis_baselines_and_children() {
        const TIP: u64 = 720;
        const WINDOW: u64 = 37;
        const SPAN: u64 = 64;
        let mut full = MemoryStore::default();
        let mut daa_by_height = Vec::with_capacity(TIP as usize + 1);
        daa_by_height.push(0u64);
        full.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=TIP {
            let daa = daa_by_height[(height - 1) as usize] + 1 + u64::from(height.is_multiple_of(17)) * 3;
            daa_by_height.push(daa);
            child(&mut full, h(height), h(height - 1), daa, WINDOW, delta(height), height.is_multiple_of(7));
        }
        assert_eq!(palw_spam_checkpoint_span(WINDOW), SPAN);

        // Reproducible pseudo-random pruning points exercise checkpoint crossings on both sides of
        // powers of two. Each imported one-span path must continue identically to the genesis store.
        let mut random = 0xd1b5_4a32_d192_ed03u64;
        for _ in 0..48 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let pp = SPAN + random % (TIP - 2 * SPAN);
            let future = 1 + random.rotate_left(19) % (SPAN + 8);

            let mut retained = MemoryStore::default();
            let path = palw_spam_retained_path(&full, h(pp), WINDOW).unwrap();
            assert_eq!(path.len(), SPAN as usize + 1);
            for (hash, state) in path {
                retained.rows.insert(hash, state);
            }

            for height in pp + 1..=pp + future {
                let args = (h(height - 1), daa_by_height[height as usize], WINDOW, delta(height), height.is_multiple_of(7));
                let expected = palw_spam_derive_child(&full, args.0, args.1, args.2, args.3, args.4).unwrap();
                let actual = palw_spam_derive_child(&retained, args.0, args.1, args.2, args.3, args.4).unwrap();
                assert_eq!(actual, expected, "PP={pp}, child height={height}");
                retained.put(h(height), actual.0);
            }

            let end = pp + future;
            let lower = daa_by_height[end as usize].saturating_sub(WINDOW);
            let expected = palw_spam_window_baseline(&full, h(end), lower, WINDOW).unwrap();
            let actual = palw_spam_window_baseline(&retained, h(end), lower, WINDOW).unwrap();
            assert_eq!(actual.as_deref(), expected.as_deref(), "PP={pp}, end={end}, lower={lower}");
        }
    }

    #[test]
    fn retained_restart_fork_and_reorg_match_from_genesis() {
        const WINDOW: u64 = 17;
        const PP: u64 = 120;
        let mut full = MemoryStore::default();
        full.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=PP {
            child(&mut full, h(height), h(height - 1), height, WINDOW, delta(height), height.is_multiple_of(5));
        }

        let mut restarted = MemoryStore::default();
        for (hash, state) in palw_spam_retained_path(&full, h(PP), WINDOW).unwrap() {
            restarted.rows.insert(hash, state);
        }

        let mut left_parent = h(PP);
        let mut right_parent = h(PP);
        for offset in 1..=48 {
            let daa = PP + offset;
            let left_hash = branch_h(1, daa);
            let right_hash = branch_h(2, daa);
            let left_delta = PalwSpamLaneDelta { hash_blues: u64::from(offset.is_multiple_of(3)), replica_blues: 0 };
            let right_delta = PalwSpamLaneDelta { hash_blues: 0, replica_blues: u64::from(offset.is_multiple_of(4)) };

            let expected_left = palw_spam_derive_child(&full, left_parent, daa, WINDOW, left_delta, offset.is_multiple_of(2)).unwrap();
            let actual_left =
                palw_spam_derive_child(&restarted, left_parent, daa, WINDOW, left_delta, offset.is_multiple_of(2)).unwrap();
            assert_eq!(actual_left, expected_left);
            full.put(left_hash, expected_left.0);
            restarted.put(left_hash, actual_left.0);
            left_parent = left_hash;

            let expected_right =
                palw_spam_derive_child(&full, right_parent, daa, WINDOW, right_delta, !offset.is_multiple_of(2)).unwrap();
            let actual_right =
                palw_spam_derive_child(&restarted, right_parent, daa, WINDOW, right_delta, !offset.is_multiple_of(2)).unwrap();
            assert_eq!(actual_right, expected_right);
            full.put(right_hash, expected_right.0);
            restarted.put(right_hash, actual_right.0);
            right_parent = right_hash;
        }

        // Simulate selecting the right branch after processing the left branch, then extending it.
        let reorg_hash = branch_h(3, PP + 49);
        let expected = palw_spam_derive_child(&full, right_parent, PP + 49, WINDOW, PalwSpamLaneDelta::default(), false).unwrap();
        let actual = palw_spam_derive_child(&restarted, right_parent, PP + 49, WINDOW, PalwSpamLaneDelta::default(), false).unwrap();
        assert_eq!(actual, expected);
        restarted.put(reorg_hash, actual.0);
        assert_ne!(restarted.rows[&reorg_hash].selected_parent, Some(left_parent));
    }

    #[test]
    fn reclaim_moves_one_boundary_at_a_time_and_preserves_side_fork_closures() {
        const WINDOW: u64 = 7;
        const SPAN: u64 = 8;
        const TIP: u64 = 48;
        let mut store = MemoryStore::default();
        store.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=TIP {
            child(&mut store, h(height), h(height - 1), height, WINDOW, delta(height), false);
        }

        for pp in 2 * SPAN..=TIP {
            let all = (0..=pp).map(h).collect::<Vec<_>>();
            let snapshot_support = (pp - SPAN..pp).map(h).collect::<Vec<_>>();
            let reclaim = palw_spam_reclaim_candidates(&store, all, [h(pp)], snapshot_support, WINDOW).unwrap();
            assert_eq!(reclaim.len() as u64, pp - SPAN, "PP={pp}");
            assert!(reclaim.iter().all(|hash| store.rows[hash].selected_height < pp - SPAN));
        }

        // A retained anticone/side-fork header is an independent seed. Its fork-local row and full
        // checkpoint closure survive even when the main-chain snapshot alone would reclaim them.
        let fork_base = 20;
        let mut parent = h(fork_base);
        for height in fork_base + 1..=fork_base + 5 {
            let hash = branch_h(9, height);
            let (state, _) = palw_spam_derive_child(&store, parent, height, WINDOW, PalwSpamLaneDelta::default(), true).unwrap();
            store.put(hash, state);
            parent = hash;
        }
        let all = store.rows.keys().copied().collect::<Vec<_>>();
        let all_len = all.len();
        let support = (TIP - SPAN..TIP).map(h).collect::<Vec<_>>();
        let reclaim = palw_spam_reclaim_candidates(&store, all, [h(TIP), parent], support, WINDOW).unwrap();
        let retained_count = all_len - reclaim.len();
        assert!(
            retained_count <= 2 * (SPAN as usize + 1) + SPAN as usize,
            "PP + one surviving side-header seed + pinned support exceeded the explicit closure bound"
        );
        assert!(!reclaim.contains(&parent));
        for height in fork_base + 1..=fork_base + 5 {
            assert!(!reclaim.contains(&branch_h(9, height)));
        }
    }

    #[test]
    fn overlapping_retained_header_closures_are_walked_near_linearly() {
        const WINDOW: u64 = 37;
        const SPAN: u64 = 64;
        const TIP: u64 = 256;
        let mut store = MemoryStore::default();
        store.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=TIP {
            child(&mut store, h(height), h(height - 1), height, WINDOW, PalwSpamLaneDelta::default(), false);
        }
        let tips = (TIP - SPAN..=TIP).map(h).collect::<Vec<_>>();
        store.reset_reads();
        let closure = palw_spam_retained_closure(&store, tips.iter().copied(), WINDOW).unwrap();
        assert_eq!(closure.len(), (2 * SPAN + 1) as usize);
        assert!(
            store.reads.get() <= 4 * closure.len() + tips.len(),
            "{} reads for {} overlapping rows regressed toward tips×span",
            store.reads.get(),
            closure.len()
        );
    }

    #[test]
    fn fresh_import_grows_then_reclaims_and_restarts_with_one_checkpoint() {
        const WINDOW: u64 = 17;
        const SPAN: u64 = 32;
        const PP: u64 = 96;
        const FUTURE: u64 = 2 * SPAN + 5;
        let mut full = MemoryStore::default();
        full.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=PP {
            child(&mut full, h(height), h(height - 1), height, WINDOW, PalwSpamLaneDelta::default(), false);
        }
        let mut imported = MemoryStore::default();
        for (hash, state) in palw_spam_retained_path(&full, h(PP), WINDOW).unwrap() {
            imported.rows.insert(hash, state);
        }
        let before = imported.rows.len();
        let all = imported.rows.keys().copied().collect::<Vec<_>>();
        let support = (PP - SPAN..PP).map(h).collect::<Vec<_>>();
        let reclaim = palw_spam_reclaim_candidates(&imported, all, [h(PP)], support, WINDOW).unwrap();
        assert!(reclaim.is_empty(), "an exact imported checkpoint must not demand a second older checkpoint");
        assert_eq!(before, SPAN as usize + 1);

        for height in PP + 1..=PP + FUTURE {
            let expected =
                palw_spam_derive_child(&full, h(height - 1), height, WINDOW, delta(height), height.is_multiple_of(6)).unwrap();
            let actual =
                palw_spam_derive_child(&imported, h(height - 1), height, WINDOW, delta(height), height.is_multiple_of(6)).unwrap();
            assert_eq!(actual, expected);
            full.put(h(height), expected.0);
            imported.put(h(height), actual.0);
        }

        // Once catch-up advances the boundary, old imported support is neither a live-header closure
        // nor part of the new snapshot and is reclaimed down to exactly one checkpoint.
        let new_pp = PP + FUTURE;
        let all = imported.rows.keys().copied().collect::<Vec<_>>();
        let support = (new_pp - SPAN..new_pp).map(h).collect::<Vec<_>>();
        let reclaim = palw_spam_reclaim_candidates(&imported, all, [h(new_pp)], support, WINDOW).unwrap();
        for hash in reclaim {
            imported.rows.remove(&hash);
        }
        assert_eq!(imported.rows.len(), SPAN as usize + 1);

        let restarted = MemoryStore { rows: imported.rows.clone(), ..Default::default() };
        let expected = palw_spam_derive_child(&full, h(new_pp), new_pp + 1, WINDOW, delta(new_pp + 1), false).unwrap();
        let actual = palw_spam_derive_child(&restarted, h(new_pp), new_pp + 1, WINDOW, delta(new_pp + 1), false).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn database_reclaim_has_a_bounded_restart_safe_growth_envelope() {
        const WINDOW: u64 = 17;
        const SPAN: u64 = 32;
        const TIP: u64 = 192;
        let mut full = MemoryStore::default();
        full.put(h(0), PalwSpamAccumulatorV1::root(0));
        for height in 1..=TIP {
            child(&mut full, h(height), h(height - 1), height, WINDOW, delta(height), height.is_multiple_of(6));
        }

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwSpamAccumulatorStore::new(db.clone(), CachePolicy::Count(512));
        let mut insert_batch = WriteBatch::default();
        for height in 0..=TIP {
            store.insert_batch(&mut insert_batch, h(height), full.rows[&h(height)].clone()).unwrap();
        }
        db.write(insert_batch).unwrap();

        let before_restart = store.clone_with_new_cache(CachePolicy::Count(8));
        let rows = before_restart.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(rows.len(), TIP as usize + 1);
        let support = (TIP - SPAN..TIP).map(h).collect::<Vec<_>>();
        let reclaim =
            palw_spam_reclaim_candidates(&before_restart, rows.iter().map(|(hash, _)| *hash), [h(TIP)], support, WINDOW).unwrap();
        assert_eq!(reclaim.len(), (TIP - SPAN) as usize);

        let mut delete_batch = WriteBatch::default();
        for hash in reclaim {
            before_restart.delete_batch(&mut delete_batch, hash).unwrap();
        }
        db.write(delete_batch).unwrap();

        let after_restart = before_restart.clone_with_new_cache(CachePolicy::Count(8));
        let remaining = after_restart.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(remaining.len(), (SPAN + 1) as usize);
        assert!(remaining.iter().all(|(_, state)| state.selected_height >= TIP - SPAN));

        let expected = palw_spam_derive_child(&full, h(TIP), TIP + 1, WINDOW, delta(TIP + 1), false).unwrap();
        let actual = palw_spam_derive_child(&after_restart, h(TIP), TIP + 1, WINDOW, delta(TIP + 1), false).unwrap();
        assert_eq!(actual, expected, "reclaim + cache restart must preserve the next child exactly");
    }

    #[test]
    fn persisted_row_size_and_raw_growth_budget_are_pinned() {
        let row = PalwSpamAccumulatorV1 {
            version: 1,
            daa_score: 3,
            selected_height: 3,
            total_hash_blues: 2,
            total_replica_blues: 1,
            selected_parent: Some(h(2)),
            skip: Some(h(1)),
        };
        let encoded = bincode::serialize(&row).unwrap();
        assert_eq!(encoded.len(), 180, "fixed row must not regress to a per-block jump vector");
        assert_eq!(encoded.len() as u64 * 10 * 86_400, 155_520_000, "10-BPS raw value bytes/day, before RocksDB overhead/pruning");
    }
}
