use std::{collections::VecDeque, ops::DerefMut, sync::Arc};

use itertools::Itertools;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
    blockhash::{BlockHashes, ORIGIN},
    header::Header,
    pruning::PruningPointProof,
    topological_order::{is_parent_first, stable_topological_order},
};
use kaspa_core::{debug, trace};
use kaspa_database::prelude::*;
use kaspa_utils::binary_heap::TopK;
use parking_lot::RwLock;

use crate::{
    model::{
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore, GhostdagStoreReader},
            headers::{HeaderStoreReader, HeaderWithBlockLevel},
            pruning::{PruningProofDescriptor, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader},
            relations::{DbRelationsStore, RelationsStoreReader},
        },
    },
    processes::{
        ghostdag::{ordering::SortableBlock, protocol::GhostdagManager},
        pruning_proof::{GhostdagReaderExt, ProofInternalError},
        reachability::inquirer as reachability,
        relations::RelationsStoreExtensions,
    },
};

use super::{ProofInternalResult, PruningProofManager};
use crate::model::services::reachability::MTReachabilityService;

#[cfg(test)]
#[path = "build_parent_order_tests.rs"]
mod parent_order_tests;

#[derive(Clone)]
struct LevelProofContext {
    ghostdag_store: Arc<DbGhostdagStore>,
    tip: BlockHash,
    root: BlockHash,
    count: u64,
}

struct ProofGraphEntry {
    header: Arc<Header>,
    parents: Vec<BlockHash>,
}

fn build_error(message: impl Into<String>) -> ProofInternalError {
    ProofInternalError::NotEnoughHeadersToBuildProof(message.into())
}

/// The priority chooses among ready nodes only; work/hash never establishes an edge.
fn order_graph(graph: &BlockHashMap<ProofGraphEntry>, reverse: bool) -> ProofInternalResult<Vec<BlockHash>> {
    let mut dependencies: BlockHashMap<Vec<BlockHash>> = graph.keys().map(|hash| (*hash, Vec::new())).collect();
    for (&hash, entry) in graph {
        for &parent in &entry.parents {
            if !graph.contains_key(&parent) {
                return Err(build_error(format!("proof graph missing parent {parent} of {hash}")));
            }
            if reverse {
                dependencies.get_mut(&parent).unwrap().push(hash);
            } else {
                dependencies.get_mut(&hash).unwrap().push(parent);
            }
        }
    }
    let mut priority = graph.keys().copied().collect_vec();
    priority.sort_by_key(|hash| SortableBlock::new(*hash, graph[hash].header.blue_work));
    if reverse {
        priority.reverse();
    }
    let order = stable_topological_order(priority, |hash| *hash, |hash| dependencies[hash].clone());
    if !is_parent_first(&order, |hash| *hash, |hash| dependencies[hash].clone()) {
        return Err(build_error("proof graph contains a parent cycle"));
    }
    Ok(order)
}

/// A relations-store reader restricted to the future of a fixed root block (including the root).
///
/// Only parents and children that lie within the root’s future are exposed.
/// This provides a consistent, root-relative view of relations when operating on
/// proofs or subgraphs confined to that region of the DAG.
#[derive(Clone)]
struct FutureIntersectRelations<T: RelationsStoreReader, U: ReachabilityService> {
    relations_store: T,
    reachability_service: U,
    root: BlockHash,
    members: Arc<BlockHashSet>,
}

impl<T: RelationsStoreReader, U: ReachabilityService> FutureIntersectRelations<T, U> {
    fn new(relations_store: T, reachability_service: U, root: BlockHash, members: Arc<BlockHashSet>) -> Self {
        Self { relations_store, reachability_service, root, members }
    }
}

impl<T: RelationsStoreReader, U: ReachabilityService> RelationsStoreReader for FutureIntersectRelations<T, U> {
    fn get_parents(&self, hash: BlockHash) -> Result<BlockHashes, StoreError> {
        self.relations_store
            .get_parents(hash)
            .map(|hashes| hashes.iter().copied().filter(|h| self.members.contains(h)).collect_vec().into())
    }

    fn get_children(&self, hash: BlockHash) -> StoreResult<ReadLock<BlockHashSet>> {
        assert!(self.reachability_service.is_dag_ancestor_of(self.root, hash), "future(root) invariant violated");
        self.relations_store.get_children(hash)
    }

    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        Ok(self.relations_store.has(hash)? && self.reachability_service.is_dag_ancestor_of(self.root, hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        unreachable!("not expected to be called in this context")
    }
}

/// Utility for creating retry-indexed temporary ghostdag stores.
///
/// Each call to `new_store` returns a fresh temporary `DbGhostdagStore` for the
/// given level, using an incrementing retry index to avoid namespace collisions.
/// This is used when multiple ghostdag attempts may be required for the same level.
struct GhostdagStoreFactory {
    db: Arc<DB>,
    cache_policy: CachePolicy,
    level: BlockLevel,
    retries: u8,
}

impl GhostdagStoreFactory {
    fn new(db: Arc<DB>, cache_policy: CachePolicy, level: BlockLevel) -> Self {
        Self { db, cache_policy, level, retries: 0 }
    }

    /// Creates a fresh temporary ghostdag store for the next retry attempt
    fn new_store(&mut self) -> Arc<DbGhostdagStore> {
        self.retries += 1;
        Arc::new(DbGhostdagStore::new_temp(self.db.clone(), self.level, self.cache_policy, self.cache_policy, self.retries - 1))
    }
}

/// Utility for creating retry-indexed temporary reachability stores.
struct ReachabilityStoreFactory {
    db: Arc<DB>,
    cache_policy: CachePolicy,
    level: BlockLevel,
    retries: u8,
}

impl ReachabilityStoreFactory {
    fn new(db: Arc<DB>, cache_policy: CachePolicy, level: BlockLevel) -> Self {
        Self { db, cache_policy, level, retries: 0 }
    }

    fn new_store(&mut self) -> Arc<RwLock<DbReachabilityStore>> {
        self.retries += 1;
        Arc::new(RwLock::new(DbReachabilityStore::with_block_level_retry(
            self.db.clone(),
            self.cache_policy,
            self.cache_policy,
            self.level,
            self.retries - 1,
        )))
    }
}

impl PruningProofManager {
    /// Collect before deriving counts/GHOSTDAG. A work-priority walk may discover
    /// an ancestor before one of its other descendants on a zero-weight lane.
    fn collect_proof_graph(
        &self,
        tip: BlockHash,
        root: Option<BlockHash>,
        level: BlockLevel,
        relations: Option<&DbRelationsStore>,
        cache: &mut BlockHashMap<Arc<Header>>,
    ) -> ProofInternalResult<BlockHashMap<ProofGraphEntry>> {
        let mut graph = BlockHashMap::new();
        let mut pending = vec![tip];
        while let Some(hash) = pending.pop() {
            if graph.contains_key(&hash) {
                continue;
            }
            if let Some(root) = root
                && !self
                    .reachability_service
                    .try_is_dag_ancestor_of(root, hash)
                    .map_err(|error| build_error(format!("proof region reachability: {error}")))?
            {
                continue; // Intentional diamond boundary, not unavailable state.
            }
            let header = if let Some(header) = cache.get(&hash) {
                header.clone()
            } else {
                let header =
                    self.headers_store.get_header(hash).map_err(|error| build_error(format!("proof header {hash}: {error}")))?;
                cache.insert(hash, header.clone());
                header
            };
            let candidates = match relations {
                Some(relations) => {
                    relations.get_parents(hash).map_err(|error| build_error(format!("proof relations {hash}: {error}")))?.to_vec()
                }
                None => self.parents_manager.parents_at_level(&header, level).to_vec(),
            };
            let mut parents = Vec::new();
            for parent in candidates {
                if parent == ORIGIN {
                    continue;
                }
                let reachable = self
                    .reachability_store
                    .read()
                    .has(parent)
                    .map_err(|error| build_error(format!("proof parent reachability {parent}: {error}")))?;
                if reachable {
                    parents.push(parent);
                    pending.push(parent);
                } else if relations.is_some() {
                    return Err(build_error(format!("temporary proof parent {parent} is unavailable")));
                } // Unreachable/pruned parents are intentionally outside a new proof.
            }
            parents.sort_unstable();
            parents.dedup();
            graph.insert(hash, ProofGraphEntry { header, parents });
        }
        if root.is_some_and(|root| !graph.contains_key(&root)) {
            return Err(build_error("proof root is absent from the collected region"));
        }
        if let Some(root) = root {
            // Global ancestry alone is insufficient: at a higher proof level a
            // block may be globally after root through lower-level edges only.
            // Preserve the old root-forward traversal's exact level-DAG region.
            let mut children: BlockHashMap<Vec<BlockHash>> = graph.keys().map(|hash| (*hash, Vec::new())).collect();
            for (&hash, entry) in &graph {
                for parent in &entry.parents {
                    if let Some(children) = children.get_mut(parent) {
                        children.push(hash);
                    }
                }
            }
            let mut connected = BlockHashSet::new();
            let mut pending = vec![root];
            while let Some(hash) = pending.pop() {
                if connected.insert(hash) {
                    pending.extend(children[&hash].iter().copied());
                }
            }
            if !connected.contains(&tip) {
                return Err(build_error("proof tip is not reachable from root at its level"));
            }
            graph.retain(|hash, _| connected.contains(hash));
        }
        let members: BlockHashSet = graph.keys().copied().collect();
        for entry in graph.values_mut() {
            entry.parents.retain(|parent| members.contains(parent));
        }
        Ok(graph)
    }

    /// Builds a pruning-point proof for `pp` by computing per-level MLS proof contexts and
    /// collecting the headers in `future(root) ∩ past(tip)` for each level.
    /// Temporary stores are used during construction, and headers are shared (via arcs)
    /// across levels in the final proof.
    pub(crate) fn try_build_pruning_point_proof(&self, pp: BlockHash) -> ProofInternalResult<PruningPointProof> {
        let descriptor = self
            .pruning_point_store
            .read()
            .pruning_proof_descriptor()
            .optional()
            .map_err(|error| build_error(format!("proof descriptor: {error}")))?;
        if let Some(descriptor) = descriptor.as_ref()
            && descriptor.pruning_point == pp
            && !descriptor.external
        {
            return self.proof_from_descriptor(descriptor);
        }
        let new_descriptor = if pp == self.genesis_hash {
            let (tips, roots, counts) = (0..=self.max_block_level).map(|_| (self.genesis_hash, self.genesis_hash, 1)).multiunzip();
            PruningProofDescriptor::new(self.genesis_hash, tips, roots, counts)
        } else {
            self.calc_new_proof(pp, descriptor.as_deref())?
        };
        let proof = self.proof_from_descriptor(&new_descriptor)?;
        self.pruning_point_store
            .write()
            .set_pruning_proof_descriptor(new_descriptor)
            .map_err(|error| build_error(format!("proof descriptor commit: {error}")))?;
        Ok(proof)
    }

    /// Reconstruct the complete diamond first, then emit it with all in-region
    /// parents ready. Header Arcs remain shared between proof levels.
    fn proof_from_descriptor(&self, descriptor: &PruningProofDescriptor) -> ProofInternalResult<PruningPointProof> {
        let levels = self.max_block_level as usize + 1;
        if descriptor.tips.len() != levels || descriptor.roots.len() != levels || descriptor.counts.len() != levels {
            return Err(build_error("proof descriptor level count mismatch"));
        }
        let mut cache = BlockHashMap::with_capacity(4 * self.pruning_proof_m as usize);
        (0..=self.max_block_level)
            .map(|level| {
                let index = level as usize;
                let graph =
                    self.collect_proof_graph(descriptor.tips[index], Some(descriptor.roots[index]), level, None, &mut cache)?;
                let order = order_graph(&graph, false)?;
                if order.len() as u64 != descriptor.counts[index] {
                    return Err(build_error(format!("rebuilt proof level {level} count differs from descriptor")));
                }
                Ok(order.into_iter().map(|hash| graph[&hash].header.clone()).collect())
            })
            .collect()
    }

    /// Computes level-proof contexts for all levels, processing levels from high to low to satisfy
    /// MLS inter-level constraints, and aggregates the results into a pruning-proof descriptor.
    fn calc_new_proof(
        &self,
        pp: BlockHash,
        prev_descriptor: Option<&PruningProofDescriptor>,
    ) -> ProofInternalResult<PruningProofDescriptor> {
        let (_db_lifetime, temp_db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let pp_header = self
            .headers_store
            .get_header_with_block_level(pp)
            .map_err(|error| build_error(format!("proof pruning point: {error}")))?;

        let mut level_proof_ctxs: Vec<Option<LevelProofContext>> = vec![None; (self.max_block_level + 1).into()];

        for level in (0..=self.max_block_level).rev() {
            let level_idx = level as usize;
            let required_block = if level != self.max_block_level {
                let LevelProofContext { ghostdag_store: next_level_gd_store, tip: next_level_tip, .. } =
                    level_proof_ctxs[level_idx + 1].as_ref().unwrap();

                let block_at_depth_m_at_next_level = next_level_gd_store
                    .block_at_depth(*next_level_tip, self.pruning_proof_m)
                    .map_err(|err| build_error(format!("next level: {}, err: {}", level + 1, err)))?;
                Some(block_at_depth_m_at_next_level)
            } else {
                None
            };
            level_proof_ctxs[level_idx] = Some(self.calc_level_proof_context(
                &pp_header,
                level,
                required_block,
                prev_descriptor.as_ref().map(|d| d.tips[level_idx]),
                prev_descriptor.as_ref().map(|d| d.roots[level_idx]),
                temp_db.clone(),
            )?);
        }

        let (tips, roots, counts) = level_proof_ctxs.into_iter().map(Option::unwrap).map(|l| (l.tip, l.root, l.count)).multiunzip();

        Ok(PruningProofDescriptor::new(pp, tips, roots, counts))
    }

    /// Computes a level-proof context by incrementally expanding the level relations subgraph and
    /// periodically attempting to anchor a proof between a candidate `root` and the selected `tip`.
    ///
    /// High-level flow:
    /// 1. Determine the selected `tip` at `level` (either the pruning point itself, or its approximate
    ///    selected parent at that level).
    /// 2. Traverse *backward* from the selected `tip` while populating a temporary relations store.
    ///    Traversal is performed in reverse-topological order so that all derived data
    ///    (e.g. future-size tracking, ghostdag inputs) is observed consistently.
    /// 3. Whenever the current block is a viable root candidate (sufficient base-level depth,
    ///    sufficient future size, and inclusion constraints), populate a temporary ghostdag store
    ///    for the region `future(root) ∩ past(tip)` and test whether it satisfies the
    ///    proof-level requirements.
    /// 4. If a candidate fails due to insufficient realized blue depth (due to reds),
    ///    increase the future-size threshold and continue searching further into the past.
    ///
    /// If `required_block` is provided, the chosen root must lie in its past.
    /// Typically, this block is the one at depth `M` from the *next* level, as mandated by the
    /// MLS (mining-in-log-space) protocol. Since level proofs are computed from higher levels
    /// to lower ones, the caller already has this block available and supplies it here to
    /// constrain root selection and ensure inter-level consistency.
    fn calc_level_proof_context(
        &self,
        pp_header: &HeaderWithBlockLevel,
        level: BlockLevel,
        required_block: Option<BlockHash>,
        prev_tip: Option<BlockHash>,
        prev_root: Option<BlockHash>,
        db: Arc<DB>,
    ) -> ProofInternalResult<LevelProofContext> {
        // Select the tip at this level:
        // - If the pruning point level >= level, use it.
        // - Otherwise, use the approximate selected parent at level.
        let tip = if pp_header.block_level >= level {
            pp_header.header.hash
        } else {
            // When advancing from a previous descriptor, require `prev_tip` to lie in the past of the new selected tip.
            // This preserves monotonicity across successive proofs (see `prev_root` rationale below).
            //
            // Note: such a parent always exists because the new pruning point is in the future of previous pruning points.
            let mut candidates = Vec::new();
            for parent in self.reachable_parents_at_level(level, &pp_header.header)? {
                if let Some(previous) = prev_tip
                    && !self
                        .reachability_service
                        .try_is_dag_ancestor_of(previous, parent)
                        .map_err(|error| build_error(format!("proof previous tip ancestry: {error}")))?
                {
                    continue;
                }
                candidates.push(
                    self.headers_store.get_header(parent).map_err(|error| build_error(format!("proof parent header: {error}")))?,
                );
            }
            candidates
                .into_iter()
                .max_by_key(|header| SortableBlock::new(header.hash, header.blue_work))
                .ok_or_else(|| build_error("no reachable parents"))?
                .hash
        };

        // Base-level blue score of the selected tip, taken directly from the header.
        // This is distinct from the *locally computed* blue score later derived from
        // the temporary ghostdag instance at this level.
        let tip_header_score =
            self.headers_store.get_blue_score(tip).map_err(|error| build_error(format!("proof tip score: {error}")))?;

        // Proof thresholds:
        // - required_future_size gates root candidacy based on how much future mass a root covers.
        // - required_base_level_depth is a base-level (header) blue-depth guard: if level 0
        //   lacks sufficient blues here, we avoid premature ghostdag attempts.
        let mut required_future_size = 2 * self.pruning_proof_m;
        let required_base_level_depth = (self.pruning_proof_m as f64 * 2.1) as u64; // ~= 2100 for M=1000

        // If no explicit required block is provided, default to `tip`.
        // Typically, `required_block` is the block at depth `M` from the *next* level, per the MLS protocol
        let required = required_block.unwrap_or(tip);

        // Collect the region before using any derived future size. Descendants
        // become dependencies, with the old maximum Sortable priority among ready nodes.
        let graph = self.collect_proof_graph(tip, prev_root, level, None, &mut BlockHashMap::new())?;
        let reverse_order = order_graph(&graph, true)?;

        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);

        // A single shared relations store is maintained for the entire search of this level.
        let mut relations_store = DbRelationsStore::new_temp(db.clone(), level, 0, cache_policy, cache_policy);

        // For each visited block, store the size of its (known) future up to `tip`.
        let mut future_sizes_map = BlockHashMap::<u64>::new();

        // Each ghostdag attempt uses a fresh temp store namespace (indexed internally by `retries`).
        let mut ghostdag_factory = GhostdagStoreFactory::new(db.clone(), cache_policy, level);
        let mut reachability_factory = ReachabilityStoreFactory::new(db.clone(), cache_policy, level);

        // Track a few high-future-size candidates for a final fallback pass
        let mut best_future_roots = TopK::<(u64, BlockHash), 8>::new();

        // Try to realize a level-proof from a candidate root
        let mut try_root = |relations_store: &DbRelationsStore,
                            root: BlockHash,
                            future_size: u64|
         -> ProofInternalResult<Option<LevelProofContext>> {
            // Populate ghostdag for `future(root) ∩ past(tip)` and test depth requirements.
            let (ghostdag_store, has_required_block, count) = self.populate_level_proof_ghostdag_data(
                relations_store,
                &mut ghostdag_factory,
                &mut reachability_factory,
                root,
                tip,
                required,
                level,
                self.ghostdag_k,
            )?;

            // Realized blue depth for this root, computed from the level-specific ghostdag
            let current_level_score =
                ghostdag_store.get_blue_score(tip).map_err(|error| build_error(format!("proof tip ghostdag: {error}")))?;

            // Log all non-trivial cases
            if tip != self.genesis_hash {
                debug!(
                    "level: {}, future: {}, blue score: {}, count: {}, retries: {}",
                    level, future_size, current_level_score, count, ghostdag_factory.retries
                );
            }

            // Success:
            // - Must include the required block (the block at depth M from the next level)
            // - If root is genesis, required-block inclusion is sufficient
            // - Otherwise require at least `2M` blue depth at this level
            if has_required_block && (root == self.genesis_hash || current_level_score >= 2 * self.pruning_proof_m) {
                Ok(Some(LevelProofContext { ghostdag_store, tip, root, count }))
            } else {
                Ok(None)
            }
        };

        for current in reverse_order {
            let entry = &graph[&current];
            let header = &entry.header;
            let parents: BlockHashes = entry.parents.clone().into();

            // Persist relations for `current`
            relations_store
                .insert(current, parents.clone())
                .map_err(|error| build_error(format!("proof relations commit: {error}")))?;

            trace!("Level: {} | Counting future size of {}", level, current);
            let future_size = self.count_future_size(&relations_store, current, &future_sizes_map)?;
            future_sizes_map.insert(current, future_size);
            trace!("Level: {} | BlockHash: {} | Future Size: {}", level, current, future_size);

            // Base-level depth from `tip`, measured using *header* blue scores.
            let base_level_depth = tip_header_score.saturating_sub(header.blue_score);

            // Root candidacy conditions:
            // - Must be in the past of `required`
            // - And one of:
            //   (a) genesis
            //   (b) sufficiently large future and sufficiently deep base-level distance
            if self
                .reachability_service
                .try_is_dag_ancestor_of(current, required)
                .map_err(|error| build_error(format!("proof required ancestry: {error}")))?
            {
                // If the root appears immediately viable, attempt ghostdag now.
                // A successful attempt requires ≥ 2M realized blues at this level.
                if current == self.genesis_hash
                    || (future_size >= required_future_size && base_level_depth >= required_base_level_depth)
                {
                    let root = current;
                    if let Some(level_ctx) = try_root(&relations_store, root, future_size)? {
                        return Ok(level_ctx);
                    }

                    // Large enough future with insufficient blue depth implies reds; increase the
                    // future-size threshold and retry further in the past.
                    required_future_size = (required_future_size as f64 * 1.1) as u64;
                } else if future_size >= 2 * self.pruning_proof_m {
                    // Minimum precondition for reaching ≥ 2M blues is future_size ≥ 2M.
                    // Defer ghostdag and keep as a fallback candidate.
                    best_future_roots.push((future_size, current));
                }
            }
        }

        // Use the previous proof's root as the fallback anchor when progressing proofs.
        // With a fixed root, ghostdag selection is deterministic, and if the new tip is in the future of the
        // previous tip then blue score/work can only increase — so once the 2M (or genesis) invariant holds,
        // it continues to hold for all future progressions.
        if let Some(root) = prev_root {
            let future_size =
                *future_sizes_map.get(&root).ok_or_else(|| build_error("previous proof root missing from reverse traversal"))?;
            if let Some(level_ctx) = try_root(&relations_store, root, future_size)? {
                return Ok(level_ctx);
            }
        }

        // Final fallback: give a last chance to a few high-future-size roots.
        // This is only needed for migrating nodes without a stored descriptor yet, and can be removed
        // once all nodes persist descriptors (along with the whole top-k fallback path).
        for (future_size, root) in best_future_roots.into_sorted_iter_ascending().collect_vec().into_iter().rev() {
            if let Some(level_ctx) = try_root(&relations_store, root, future_size)? {
                return Ok(level_ctx);
            }
        }

        Err(build_error(format!("no sufficient root for level {level} after exhausting known headers")))
    }

    /// Given a current hash, count the blocks in its future.
    ///
    /// The algorithm works as follows:
    /// 1. Identify the dominant child (the one with the largest future) to minimize traversal,
    ///    since most of the future is expected to be covered by it.
    /// 2. Perform a BFS over all other children and their futures, skipping blocks that are
    ///    already in the future of the dominant child.
    ///
    /// This is conceptually similar to mergeset calculation logic
    /// (effectively a traversal over the reversed mergeset).
    ///
    /// Assumes `future_sizes` is populated for all children of `current` (caller is expected to be doing a topological BFS).
    fn count_future_size(
        &self,
        relations: &DbRelationsStore,
        current: BlockHash,
        future_sizes: &BlockHashMap<u64>,
    ) -> ProofInternalResult<u64> {
        // Seed the BFS queue with all children of the current hash
        let mut queue: VecDeque<_> = relations
            .get_children(current)
            .map_err(|error| build_error(format!("proof children: {error}")))?
            .read()
            .iter()
            .copied()
            .collect();
        let mut visited = BlockHashSet::new();

        struct Entry {
            child: BlockHash,
            fut_size: u64,
        }

        let mut entries = Vec::with_capacity(queue.len());
        for child in queue.iter().copied() {
            let fut_size =
                *future_sizes.get(&child).ok_or_else(|| build_error(format!("future size missing for child {child} of {current}")))?;
            entries.push(Entry { child, fut_size });
        }
        let dominant_entry = entries.into_iter().max_by_key(|entry| entry.fut_size);

        let mut count = 0;

        if let Some(Entry { child: dominant_child, fut_size }) = dominant_entry {
            // Fully account for the dominant child future (+1 for itself) and exclude it from the traversal
            count += fut_size + 1;
            visited.insert(dominant_child);

            while let Some(hash) = queue.pop_front() {
                if !visited.insert(hash) {
                    continue;
                }

                // Skip blocks that are already in the future of the dominant child
                if self
                    .reachability_service
                    .try_is_dag_ancestor_of(dominant_child, hash)
                    .map_err(|error| build_error(format!("proof future ancestry: {error}")))?
                {
                    continue;
                }

                count += 1;
                for &child in relations
                    .get_children(hash)
                    .map_err(|error| build_error(format!("proof descendant children: {error}")))?
                    .read()
                    .iter()
                {
                    queue.push_back(child);
                }
            }
        }

        trace!("Counted future size of {} as {}", current, count);
        Ok(count)
    }

    /// Forward-traverses from `root` toward `tip`, and inserts ghostdag data for each visited block.
    ///
    /// Traversal is restricted to `future(root) ∩ past(tip)` (i.e., blocks in the antipast of `tip` are ignored).
    /// Returns `true` iff `required_block` was encountered during traversal.
    fn populate_level_proof_ghostdag_data(
        &self,
        relations_store: &DbRelationsStore,
        ghostdag_factory: &mut GhostdagStoreFactory,
        reachability_factory: &mut ReachabilityStoreFactory,
        root: BlockHash,
        tip: BlockHash,
        required_block: BlockHash,
        level: BlockLevel,
        ghostdag_k: KType,
    ) -> ProofInternalResult<(Arc<DbGhostdagStore>, bool, u64)> {
        debug!("Populating GD for root {} at level {} (retry {})", root, level, ghostdag_factory.retries.saturating_sub(1));

        let graph = self.collect_proof_graph(tip, Some(root), level, Some(relations_store), &mut BlockHashMap::new())?;
        let order = order_graph(&graph, false)?;

        let ghostdag_store = ghostdag_factory.new_store();
        let reachability_store = reachability_factory.new_store();
        let reachability_service = MTReachabilityService::new(reachability_store.clone());

        // Init reachability with ORIGIN and add root as its only child
        reachability::init(reachability_store.write().deref_mut()).unwrap();
        reachability::add_block(reachability_store.write().deref_mut(), root, ORIGIN, &mut [].into_iter()).unwrap();

        // Restrict relations to `future(root)` via level reachability
        let relations_view = FutureIntersectRelations::new(
            relations_store,
            reachability_service.clone(),
            root,
            Arc::new(graph.keys().copied().collect()),
        );

        // Create a ghostdag manager over the restricted relations view
        let ghostdag_manager = GhostdagManager::with_level(
            root,
            ghostdag_k,
            ghostdag_store.clone(),
            &relations_view,
            self.headers_store.clone(),
            reachability_service.clone(),
            level,
            self.max_block_level,
            // ADR-0060: ε work for heartbeat blocks at every proof level too.
            self.palw_heartbeat_lane,
            // ADR-0068 Phase 1 (F2): and the attempt constant, for the same reason.
            self.palw_attempt_work_lane,
            // ADR-0105: and the coloring rule, for the same reason.
            self.palw_heartbeat_transparent,
            // ADR-0125: and the round lane's rule.
            self.palw_round_lane,
        )
        .with_dag_width(self.dag_width);

        // No need to initialize origin since we have a single root
        ghostdag_store.insert(root, Arc::new(ghostdag_manager.genesis_ghostdag_data())).unwrap();

        // All included parents must be present before either GHOSTDAG or the
        // temporary reachability view is asked about them.
        let mut count = 1;
        let mut has_required_block = root == required_block;
        let mut reachability_tip = root;
        for current in order.into_iter().filter(|hash| *hash != root) {
            has_required_block |= current == required_block;
            count += 1;

            let parents =
                relations_view.get_parents(current).map_err(|error| build_error(format!("proof GHOSTDAG parents: {error}")))?;
            if parents.is_empty()
                || parents.len() != graph[&current].parents.len()
                || !parents.iter().all(|parent| graph[&current].parents.contains(parent))
            {
                return Err(build_error(format!("temporary proof parent set unavailable for {current}")));
            }

            let ghostdag_data = Arc::new(ghostdag_manager.ghostdag(parents.as_slice()));
            ghostdag_store.insert(current, ghostdag_data.clone()).unwrap();

            reachability_tip = ghostdag_manager.find_selected_parent([reachability_tip, current]);

            let mut level_reachability = reachability_store.write();
            let mut reachability_mergeset = ghostdag_data
                .unordered_mergeset_without_selected_parent()
                .filter(|hash| level_reachability.has(*hash).unwrap())
                .collect_vec()
                .into_iter();

            reachability::add_block(
                level_reachability.deref_mut(),
                current,
                ghostdag_data.selected_parent,
                &mut reachability_mergeset,
            )
            .unwrap();

            if current == reachability_tip {
                reachability::hint_virtual_selected_parent(level_reachability.deref_mut(), current).unwrap();
            }
            drop(level_reachability);
        }

        // Returned for sanity testing by the caller
        Ok((ghostdag_store, has_required_block, count))
    }

    /// Returns the header's parents at `level` that are reachable according to the reachability service,
    /// i.e., parents for which reachability data exists in the database.
    ///
    /// This function enforces the reachability / storage invariants described in the
    /// [crate-level documentation](crate): only parents with reachability data are returned.
    /// By convention, the returned hashes are therefore also guaranteed to have a header
    /// entry in the database.
    fn reachable_parents_at_level(&self, level: u8, header: &Header) -> ProofInternalResult<Vec<BlockHash>> {
        let mut parents = Vec::new();
        for &parent in self.parents_manager.parents_at_level(header, level) {
            if self.reachability_store.read().has(parent).map_err(|error| build_error(format!("proof reachable parent: {error}")))? {
                parents.push(parent);
            }
        }
        Ok(parents)
    }
}
