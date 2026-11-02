use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet, hash_map::Entry::Vacant},
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockLevel;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    errors::pruning::{PruningImportError, PruningImportResult},
    header::Header,
    pruning::PruningPointProof,
    topological_order::{is_parent_first, stable_topological_order},
    trusted::TrustedBlock,
};
use kaspa_core::{debug, trace};
use kaspa_pow::calc_block_level_layer0;
use kaspa_utils::{binary_heap::BinaryHeapExtensions, vec::VecExtensions};
use rayon::prelude::*;
use rocksdb::WriteBatch;

use crate::{
    model::{
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{GhostdagData, GhostdagStore, GhostdagStoreReader},
            headers::{HeaderStore, HeaderStoreReader},
            pruning::{PruningProofDescriptor, PruningStoreReader},
            reachability::StagingReachabilityStore,
            relations::StagingRelationsStore,
            selected_chain::SelectedChainStore,
            virtual_state::VirtualState,
        },
    },
    processes::{
        ghostdag::{mergeset::unordered_mergeset_without_selected_parent, ordering::SortableBlock},
        reachability::inquirer as reachability,
        relations::RelationsStoreExtensions,
    },
};

use super::PruningProofManager;

impl PruningProofManager {
    /// Inspect only the applied proof's complete level-0 chain. The import
    /// reachability tree spans the union of every proof level and is not the
    /// GHOSTDAG selected-parent chain. Normal block statuses are not created by
    /// proof import either, so neither public block-query gate is suitable here.
    pub fn chain_contains_checkpoint(&self, point: BlockHash, checkpoint: BlockHash, daa_score: u64) -> PruningImportResult<bool> {
        let unavailable = |message: String| PruningImportError::PruningPointImportContextUnavailable(point, message);
        let pruning = self.pruning_point_store.read();
        if pruning.pruning_point().map_err(|e| unavailable(format!("checkpoint target: {e}")))? != point {
            return Err(unavailable("checkpoint target is not the applied pruning point".into()));
        }
        let descriptor = pruning.pruning_proof_descriptor().map_err(|e| unavailable(format!("checkpoint descriptor: {e}")))?;
        let (Some(&root), Some(&tip), Some(&count)) = (descriptor.roots.first(), descriptor.tips.first(), descriptor.counts.first())
        else {
            return Err(unavailable("checkpoint proof has no level-0 descriptor".into()));
        };
        if descriptor.pruning_point != point || tip != point || count == 0 {
            return Err(unavailable("checkpoint proof descriptor does not name the applied point".into()));
        }
        // A truncated proof may recompute GHOSTDAG on an incomplete history.
        // Only a genesis-rooted level 0 has the complete DAG needed to judge
        // historical selected-chain checkpoints (and exact work validation).
        if root != self.genesis_hash {
            return Err(unavailable(format!(
                "historical checkpoint chain requires a genesis-rooted level-0 proof: observed root {root}, expected genesis {}",
                self.genesis_hash
            )));
        }
        let read_header =
            |hash| self.headers_store.get_header(hash).map_err(|e| unavailable(format!("checkpoint header {hash}: {e}")));
        let checkpoint_header = read_header(checkpoint)?;
        if checkpoint_header.daa_score != daa_score {
            return Err(unavailable("checkpoint header differs from its pre-checked DAA score".into()));
        }
        let root_header = read_header(root)?;
        let point_header = read_header(point)?;
        if checkpoint_header.daa_score < root_header.daa_score || checkpoint_header.daa_score > point_header.daa_score {
            return Err(unavailable("checkpoint lies outside the applied proof's level-0 window".into()));
        }
        // The pre-check established checkpoint containment. Missing local data
        // after import must not become a peer's checkpoint violation.
        self.ghostdag_store
            .get_selected_parent(checkpoint)
            .map_err(|e| unavailable(format!("checkpoint ghostdag {checkpoint}: {e}")))?;
        let mut seen = BlockHashSet::new();
        let mut current = point;
        let mut found = false;
        loop {
            if !seen.insert(current) || seen.len() as u64 > count {
                return Err(unavailable("checkpoint selected-parent chain cycles or exceeds the proof's level-0 count".into()));
            }
            read_header(current)?;
            let parent = self
                .ghostdag_store
                .get_selected_parent(current)
                .map_err(|e| unavailable(format!("checkpoint chain {current}: {e}")))?;
            found |= current == checkpoint;
            if current == root {
                if !parent.is_origin() {
                    return Err(unavailable("checkpoint proof root has an out-of-window selected parent".into()));
                }
                return Ok(found);
            }
            if parent.is_origin() {
                return Err(unavailable("checkpoint selected-parent chain ends before the proof root".into()));
            }
            current = parent;
        }
    }

    pub fn apply_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        // Following validation of a pruning proof, various consensus storages must be updated

        let pruning_point_header = proof[0].last().unwrap().clone();
        let pruning_point = pruning_point_header.hash;

        // Build the descriptor based on the new proof before modifying it
        let descriptor = PruningProofDescriptor::from_proof(&proof, pruning_point, true);

        // Create a copy of the proof, since we're going to be mutating the proof passed to us
        let proof_sets = (0..=self.max_block_level)
            .map(|level| BlockHashSet::from_iter(proof[level as usize].iter().map(|header| header.hash)))
            .collect_vec();

        let mut expanded_proof = proof;
        let mut trusted_gd_map: BlockHashMap<GhostdagData> = BlockHashMap::new();
        // This loop expands the proof with the headers of the trusted set
        // and creates a hash to ghostdag data map of the trusted set
        // Gate trusted-set headers before computing PoW, in order, so the first shape failure wins.
        for tb in trusted_set.iter() {
            self.check_proof_header_shape(&tb.block.header, 0)?;
        }
        // Every gated header is consumed below; compute its block level in input order.
        let trusted_levels = self.batched_block_levels(trusted_set.iter().map(|tb| &tb.block.header));

        for (tb, &tb_block_level) in trusted_set.iter().zip(trusted_levels.iter()) {
            trusted_gd_map.insert(tb.block.hash(), tb.ghostdag.clone().into());

            (0..=tb_block_level).for_each(|current_proof_level| {
                // If this block was in the original proof, ignore it
                if proof_sets[current_proof_level as usize].contains(&tb.block.hash()) {
                    return;
                }
                // otherwise, add this block to the proof data
                expanded_proof[current_proof_level as usize].push(tb.block.header.clone());
            });
        }
        // Zero-weight PALW parents and children may tie on blue work. Sorting
        // by work alone is not topological, especially after appending trusted
        // headers. Do not let the level-0 ancestor filter silently omit a parent.
        self.order_expanded_proof(&mut expanded_proof)?;

        self.populate_reachability_and_headers(&expanded_proof)?;

        // sanity check
        {
            let reachability_read = self.reachability_store.read();
            for tb in trusted_set.iter() {
                // A trusted block not in the past of the pruning point is in its anticone and thus must have a body
                if tb.block.is_header_only() && !reachability_read.is_dag_ancestor_of(tb.block.hash(), pruning_point) {
                    return Err(PruningImportError::PruningPointAnticoneMissingBody(tb.block.hash()));
                }

                // Trusted blocks are expected to be in the pruning point anti-future.
                if tb.block.hash() != pruning_point && reachability_read.is_dag_ancestor_of(pruning_point, tb.block.hash()) {
                    return Err(PruningImportError::TrustedBlockInPruningPointFuture(tb.block.hash(), pruning_point));
                }
            }
        }
        // Populate ghostdag_store and relation store for every block in the proof
        trace!("Applying level 0 from the pruning point proof");
        // We are only interested in those ancestors that belong to the pruning proof,
        // so other parents are filtered out.
        // Since the dag is topologically sorted, we can construct the ancestors
        // on the fly rather than constructing it ahead of time
        let mut ancestors: HashSet<BlockHash> = HashSet::new();
        ancestors.insert(ORIGIN);

        for header in expanded_proof[0].iter() {
            let parents = Arc::new(
                self.parents_manager
                    .parents_at_level(header, 0)
                    .iter()
                    .copied()
                    .filter(|parent| ancestors.contains(parent))
                    .collect_vec()
                    .push_if_empty(ORIGIN),
            );

            self.relations_store.write().insert(header.hash, parents.clone()).unwrap();
            let gd = if let Some(gd) = trusted_gd_map.get(&header.hash) {
                gd.clone()
            } else {
                let calculated_gd = self.ghostdag_manager.ghostdag(&parents);
                // Override the ghostdag data with the real blue score and blue work
                GhostdagData {
                    blue_score: header.blue_score,
                    blue_work: header.blue_work,
                    selected_parent: calculated_gd.selected_parent,
                    mergeset_blues: calculated_gd.mergeset_blues,
                    mergeset_reds: calculated_gd.mergeset_reds,
                    blues_anticone_sizes: calculated_gd.blues_anticone_sizes,
                }
            };
            self.ghostdag_store.insert(header.hash, Arc::new(gd)).unwrap();

            ancestors.insert(header.hash);
        }

        // All fallible proof/trusted-set checks precede the authority writer.
        // Stable=false, descriptor and virtual/chain replacement commit together.
        let mut pruning = self.pruning_point_store.write();
        let mut meta = self._pruning_meta_stores.write();
        let mut batch = WriteBatch::default();
        meta.set_pruning_utxoset_stable_flag(&mut batch, false).map_err(|e| {
            PruningImportError::PruningPointImportContextUnavailable(pruning_point, format!("proof incomplete marker: {e}"))
        })?;
        pruning.set_pruning_proof_descriptor_batch(&mut batch, descriptor).unwrap();

        // Update virtual state based on proof derived pruning point.
        // updating of the utxoset is done separately as it requires downloading the new utxoset in its entirety.
        let virtual_parents = vec![pruning_point];
        let virtual_state = Arc::new(VirtualState {
            parents: virtual_parents.clone(),
            ghostdag_data: self.ghostdag_manager.ghostdag(&virtual_parents),
            ..VirtualState::default()
        });
        self.virtual_stores.write().state.set_batch(&mut batch, virtual_state).unwrap();
        self.body_tips_store.write().init_batch(&mut batch, &virtual_parents).unwrap();
        self.headers_selected_tip_store
            .write()
            .set_batch(&mut batch, SortableBlock { hash: pruning_point, blue_work: pruning_point_header.blue_work })
            .unwrap();
        self.selected_chain_store.write().init_with_pruning_point(&mut batch, pruning_point).unwrap();
        self.depth_store.insert_batch(&mut batch, pruning_point, ORIGIN, ORIGIN).unwrap();
        self.db.write(batch).unwrap();
        drop(meta);
        drop(pruning);

        Ok(())
    }

    /// Preserve the existing work order wherever it already respects parent
    /// dependencies. This changes import construction order, not GHOSTDAG order.
    fn order_expanded_proof(&self, proof: &mut PruningPointProof) -> PruningImportResult<()> {
        let point = proof.first().and_then(|level| level.last()).map_or(ORIGIN, |header| header.hash);
        for (level, headers) in proof.iter_mut().enumerate() {
            let mut ordered = std::mem::take(headers);
            ordered.sort_by(|a, b| a.blue_work.cmp(&b.blue_work));
            let parents = |header: &Arc<Header>| self.parents_manager.parents_at_level(header, level as BlockLevel).to_vec();
            ordered = stable_topological_order(ordered, |header| header.hash, parents);
            if !is_parent_first(&ordered, |header| header.hash, parents) {
                return Err(PruningImportError::PruningPointImportContextUnavailable(
                    point,
                    format!("proof level {level} has cyclic parent dependencies"),
                ));
            }
            *headers = ordered;
        }
        Ok(())
    }

    /// Block levels for `headers`, in order, computed in bounded parallel batches.
    ///
    /// Callers must gate every header first to enforce the network's algorithm and shape rules.
    fn batched_block_levels<'a>(&self, headers: impl Iterator<Item = &'a Arc<Header>>) -> Vec<BlockLevel> {
        let headers = headers.collect_vec();
        let batch = super::HEADER_POW_BATCH_SIZE;
        let mut levels = Vec::with_capacity(headers.len());
        for chunk in headers.chunks(batch) {
            // One header is the overwhelmingly common case — every network whose PoW is a hash —
            // so keep it off the thread pool entirely.
            if chunk.len() == 1 {
                levels.push(calc_block_level_layer0(chunk[0], &self.network_id, self.max_block_level));
            } else {
                levels.extend(
                    chunk.par_iter().map(|h| calc_block_level_layer0(h, &self.network_id, self.max_block_level)).collect::<Vec<_>>(),
                );
            }
        }
        levels
    }

    /// Gates every DISTINCT header of `proof` in walk order, then computes their block levels in
    /// batches. First gate failure wins, exactly as it did when the gate lived inside the walk.
    fn gated_block_levels(&self, proof: &PruningPointProof) -> PruningImportResult<BlockHashMap<BlockLevel>> {
        let mut seen = BlockHashSet::new();
        let mut distinct = Vec::new();
        for header in proof.iter().flatten() {
            if seen.insert(header.hash) {
                self.check_proof_header_shape(header, 0)?;
                distinct.push(header);
            }
        }
        let levels = self.batched_block_levels(distinct.iter().copied());
        Ok(distinct.iter().map(|h| h.hash).zip(levels).collect())
    }

    pub fn populate_reachability_and_headers(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        let capacity_estimate = self.estimate_proof_unique_size(proof);
        let mut dag = BlockHashMap::with_capacity(capacity_estimate);
        let mut up_heap = BinaryHeap::with_capacity(capacity_estimate);
        // Re-gate before recomputing PoW: this public method may be called without prior validation.
        // Hoisting the gate preserves error order. A proof that fails the gate
        // now aborts having written NOTHING to the headers store, where before it had written every
        // header up to the bad one.
        let levels = self.gated_block_levels(proof)?;
        for header in proof.iter().flatten().cloned() {
            if let Vacant(e) = dag.entry(header.hash) {
                let mut parents = BlockHashSet::with_capacity(header.direct_parents().len() * 2);
                // We collect all available parent relations in order to maximize reachability information.
                // By taking into account parents from all levels we ensure that the induced DAG has valid
                // reachability information for each level-specific sub-DAG -- hence a single reachability
                // oracle can serve them all
                for level in 0..=self.max_block_level {
                    for parent in self.parents_manager.parents_at_level(&header, level) {
                        parents.insert(*parent);
                    }
                }

                struct DagEntry {
                    header: Arc<Header>,
                    parents: Arc<BlockHashSet>,
                }

                up_heap.push(Reverse(SortableBlock { hash: header.hash, blue_work: header.blue_work }));
                e.insert(DagEntry { header, parents: Arc::new(parents) });
            }
        }

        debug!("Estimated proof size: {}, actual size: {}", capacity_estimate, dag.len());

        // The heap's (work, hash) order can put an equal-work child before its
        // parent. Reachability queries require every in-proof parent to exist,
        // including parent relations contributed by higher levels.
        let ordered = stable_topological_order(
            up_heap.into_sorted_iter().map(|block| block.0.hash).collect(),
            |hash| *hash,
            |hash| dag[hash].parents.iter().copied().collect::<Vec<_>>(),
        );
        if !is_parent_first(&ordered, |hash| *hash, |hash| dag[hash].parents.iter().copied().collect::<Vec<_>>()) {
            return Err(PruningImportError::PruningPointImportContextUnavailable(
                ordered.first().copied().unwrap_or(ORIGIN),
                "proof reachability has cyclic parent dependencies".into(),
            ));
        }

        for hash in ordered {
            let dag_entry = dag.get(&hash).unwrap();
            self.headers_store.insert(hash, dag_entry.header.clone(), levels[&hash]).unwrap();

            // Filter only existing parents
            let parents_in_dag = BinaryHeap::from_iter(
                dag_entry
                    .parents
                    .iter()
                    .cloned()
                    .filter(|parent| dag.contains_key(parent))
                    .map(|parent| SortableBlock { hash: parent, blue_work: dag.get(&parent).unwrap().header.blue_work }),
            );

            let reachability_read = self.reachability_store.upgradable_read();

            // Find the maximal parent antichain. With equal work, hash order
            // can visit an ancestor before its descendant, so remove previously
            // visited ancestors too instead of assuming descendants arrive first.
            let ancestor = |this, queried| {
                reachability_read.try_is_dag_ancestor_of(this, queried).map_err(|error| {
                    PruningImportError::PruningPointImportContextUnavailable(
                        hash,
                        format!("reachability parent query {this} -> {queried}: {error:?}"),
                    )
                })
            };
            let mut reachability_parents: Vec<SortableBlock> = Vec::new();
            for parent in parents_in_dag.into_sorted_iter() {
                let mut covered = false;
                for existing in &reachability_parents {
                    if ancestor(parent.hash, existing.hash)? {
                        covered = true;
                        break;
                    }
                }
                if covered {
                    continue;
                }
                let mut i = 0;
                while i < reachability_parents.len() {
                    if ancestor(reachability_parents[i].hash, parent.hash)? {
                        reachability_parents.remove(i);
                    } else {
                        i += 1;
                    }
                }
                reachability_parents.push(parent);
            }
            let reachability_parents_hashes =
                BlockHashes::new(reachability_parents.iter().map(|parent| parent.hash).collect_vec().push_if_empty(ORIGIN));
            let selected_parent = reachability_parents.iter().max().map(|parent| parent.hash).unwrap_or(ORIGIN);

            // Prepare batch
            let mut batch = WriteBatch::default();
            let mut reachability_relations_write = self.reachability_relations_store.write();
            let mut staging_reachability = StagingReachabilityStore::new(reachability_read);
            let mut staging_reachability_relations = StagingRelationsStore::new(&mut reachability_relations_write);

            // Stage
            staging_reachability_relations.insert(hash, reachability_parents_hashes.clone()).unwrap();
            let mergeset = unordered_mergeset_without_selected_parent(
                &staging_reachability_relations,
                &staging_reachability,
                selected_parent,
                &reachability_parents_hashes,
            );
            reachability::add_block(&mut staging_reachability, hash, selected_parent, &mut mergeset.iter().copied()).unwrap();

            // Commit
            let reachability_write = staging_reachability.commit(&mut batch).unwrap();
            staging_reachability_relations.commit(&mut batch).unwrap();

            // Write
            self.db.write(batch).unwrap();

            // Drop
            drop(reachability_write);
            drop(reachability_relations_write);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "apply_parent_order_tests.rs"]
mod parent_order_tests;
