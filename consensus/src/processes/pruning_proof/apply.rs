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
    blockhash::{BlockHashes, ORIGIN},
    errors::pruning::{PruningImportError, PruningImportResult},
    header::Header,
    pruning::PruningPointProof,
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
            ghostdag::{GhostdagData, GhostdagStore},
            headers::HeaderStore,
            pruning::PruningProofDescriptor,
            reachability::StagingReachabilityStore,
            relations::StagingRelationsStore,
            selected_chain::SelectedChainStore,
            virtual_state::{VirtualState, VirtualStateStore},
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
        // Gate every peer-supplied trusted-set header BEFORE any PoW is computed (audit P0-1 /
        // P0-2): the trusted set arrives with the proof and reaches the finalizer here, whose PALW
        // arm turns a missing worker into a panic on a non-PALW network. Sequential and
        // first-failure, so the error a given trusted set produces is exactly what it was.
        for tb in trusted_set.iter() {
            self.check_proof_header_shape(&tb.block.header, 0)?;
        }
        // ADR-0041 Decision 2. On a PALW network each of these levels is a full LLM inference. The
        // loop below has no early exit once the gate above has passed, so every gated header's PoW
        // is computed either way: batching them is the same work in less wall clock, with no
        // speculation at all — unlike the validator's own header loop, where a PoW-derived error
        // can stop the walk and the batch has to be bounded to bound the waste.
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
        // topologically sort every level in the proof
        expanded_proof.iter_mut().for_each(|level_proof| {
            level_proof.sort_by(|a, b| a.blue_work.cmp(&b.blue_work));
        });

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

        // Once applied, store the descriptor
        self.pruning_point_store.write().set_pruning_proof_descriptor(descriptor).unwrap();

        // Update virtual state based on proof derived pruning point.
        // updating of the utxoset is done separately as it requires downloading the new utxoset in its entirety.
        let virtual_parents = vec![pruning_point];
        let virtual_state = Arc::new(VirtualState {
            parents: virtual_parents.clone(),
            ghostdag_data: self.ghostdag_manager.ghostdag(&virtual_parents),
            ..VirtualState::default()
        });
        self.virtual_stores.write().state.set(virtual_state).unwrap();

        let mut batch = WriteBatch::default();
        self.body_tips_store.write().init_batch(&mut batch, &virtual_parents).unwrap();
        self.headers_selected_tip_store
            .write()
            .set_batch(&mut batch, SortableBlock { hash: pruning_point, blue_work: pruning_point_header.blue_work })
            .unwrap();
        self.selected_chain_store.write().init_with_pruning_point(&mut batch, pruning_point).unwrap();
        self.depth_store.insert_batch(&mut batch, pruning_point, ORIGIN, ORIGIN).unwrap();
        self.db.write(batch).unwrap();

        Ok(())
    }

    /// Block levels for `headers`, in order, computed in bounded parallel batches.
    ///
    /// Callers must have gated every header first: this runs the Layer-1 finalizer, whose PALW arm
    /// panics the node on an unusable runtime, and whose unknown-algo path is only total because
    /// the gate rejects a peer-chosen id before it gets here.
    fn batched_block_levels<'a>(&self, headers: impl Iterator<Item = &'a Arc<Header>>) -> Vec<BlockLevel> {
        let headers = headers.collect_vec();
        let batch = kaspa_pow::palw::inference_concurrency();
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
        // pow passing has already been checked during validation, and the trusted set was gated in
        // `apply_proof` before it was folded into `proof` here — but re-gate before these PoW
        // recomputes anyway (audit P0-1 / P0-2): this method is `pub`, and a future caller reaching
        // it without prior validation must not hand a peer-chosen algo id to the finalizer's
        // panicking PALW arm.
        //
        // Gating and computing here rather than inside the walk is ADR-0041 Decision 2: the walk's
        // only error is this gate, so hoisting it changes no error, and the levels it needs are
        // pure functions of the headers — so they batch. It also means a proof that fails the gate
        // now aborts having written NOTHING to the headers store, where before it had written every
        // header up to the bad one.
        let levels = self.gated_block_levels(proof)?;
        for header in proof.iter().flatten().cloned() {
            if let Vacant(e) = dag.entry(header.hash) {
                let block_level = levels[&header.hash];
                self.headers_store.insert(header.hash, header.clone(), block_level).unwrap();

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

        for reverse_sortable_block in up_heap.into_sorted_iter() {
            // TODO: Convert to into_iter_sorted once it gets stable
            let hash = reverse_sortable_block.0.hash;
            let dag_entry = dag.get(&hash).unwrap();

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

            // Find the maximal parent antichain from the possibly redundant set of existing parents
            let mut reachability_parents: Vec<SortableBlock> = Vec::new();
            for parent in parents_in_dag.into_sorted_iter() {
                if reachability_read.is_dag_ancestor_of_any(parent.hash, &mut reachability_parents.iter().map(|parent| parent.hash)) {
                    continue;
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
