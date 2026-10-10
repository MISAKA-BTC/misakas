use indexmap::{IndexMap, map::Entry::Occupied};
use kaspa_consensus_core::BlockHash; // PR-9.5e: block hashes are Hash64
use kaspa_consensus_core::{
    api::{BlockValidationFuture, BlockValidationFutures},
    block::Block,
    config::params::ForkActivation,
};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusProxy};
use kaspa_core::debug;
use rand::Rng;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    iter::once,
};

use super::process_queue::ProcessQueue;

/// **A block's dependencies: its direct parents and, past the EXEC payload fence (RFC-0008 v2), the lane heads its coinbase anchors.**
/// Both must be known before the block can be judged, so the orphan pool treats them alike — where the fence is in force at the block's
/// own DAA, the predicate the body stage asks before it names a missing head. Elsewhere (every shipped preset, every height below an
/// armed fence) a coinbase is the miner's bytes and is never read: the X8R review found the unconditional read let any miner end its
/// tag in a trailer naming hashes nobody has, which held its block in this pool forever on an upgraded node while an older node
/// released it — a relay split with the fence off.
fn block_deps(block: &Block, exec_v2: Option<ForkActivation>) -> Vec<BlockHash> {
    let heads = if exec_v2.is_some_and(|fence| fence.is_active(block.header.daa_score)) {
        kaspa_consensus_core::palw_exec_v2_anchor::palw_exec_v2_anchor_heads_of_block(block)
    } else {
        Vec::new()
    };
    block.header.direct_parents().iter().copied().chain(heads).collect()
}

/// The output of an orphan pool block query
#[derive(Debug)]
pub enum OrphanOutput {
    /// Block is orphan with the provided missing roots
    Roots(Vec<BlockHash>),
    /// Block has no missing roots (but it might have known orphan ancestors which are returned
    /// along with their corresponding consensus processing tasks)
    NoRoots(BlockProcessingBatch),
    /// The block does not exist in the orphan pool
    Unknown,
}

#[derive(Debug)]
enum FindRootsOutput {
    /// Block is orphan with the provided missing roots and a possible set of known orphan ancestors
    Roots(Vec<BlockHash>, HashSet<BlockHash>),
    /// Block has no missing roots (but it might have known orphan ancestors)
    NoRoots(HashSet<BlockHash>),
}

struct OrphanBlock {
    /// The actual block
    block: Block,

    /// A set of child orphans loosely maintained such that any block in the
    /// orphan pool which has this block as a direct parent will be in the set, however
    /// items are never removed, so this set might contain evicted hashes as well
    children: HashSet<BlockHash>,
}

impl OrphanBlock {
    fn new(block: Block, children: HashSet<BlockHash>) -> Self {
        Self { block, children }
    }
}

pub struct OrphanBlocksPool {
    /// NOTES:
    /// 1. We use IndexMap for cheap random eviction
    /// 2. We avoid the custom block hasher since this pool is pre-validation storage
    orphans: IndexMap<BlockHash, OrphanBlock>,
    /// Max number of orphans to keep in the pool
    max_orphans: usize,
    /// The log base 2 of `max_orphans`
    max_orphans_log: usize,
    /// RFC-0008 v2: `Params::palw_exec_payload_v2_fence` ([`block_deps`]). `None` on every shipped preset.
    exec_v2: Option<ForkActivation>,
}

impl OrphanBlocksPool {
    pub fn new(max_orphans: usize) -> Self {
        Self {
            orphans: IndexMap::with_capacity(max_orphans),
            max_orphans,
            max_orphans_log: (max_orphans as f64).log2().ceil() as usize,
            exec_v2: None,
        }
    }

    /// RFC-0008 v2: the EXEC payload's fence, so an anchoring block waits for its lane heads past it (and nothing is read below it).
    pub fn with_exec_v2(mut self, exec_v2: Option<ForkActivation>) -> Self {
        self.exec_v2 = exec_v2;
        self
    }

    /// Adds the provided block to the orphan pool. Returns None if the block is already
    /// in the pool or if the pool chose not to keep it for any reason
    pub async fn add_orphan(&mut self, consensus: &ConsensusProxy, orphan_block: Block) -> Option<OrphanOutput> {
        let orphan_hash = orphan_block.hash();
        if self.orphans.contains_key(&orphan_hash) {
            return None;
        }
        orphan_block.asses_for_cache()?;
        let (roots, orphan_ancestors) =
            match self.get_orphan_roots(consensus, block_deps(&orphan_block, self.exec_v2).into_iter().collect()).await {
                FindRootsOutput::Roots(roots, orphan_ancestors) => (roots, orphan_ancestors),
                FindRootsOutput::NoRoots(orphan_ancestors) => {
                    let blocks: Vec<_> =
                        orphan_ancestors.into_iter().map(|h| self.orphans.swap_remove(&h).expect("orphan ancestor").block).collect();
                    return Some(OrphanOutput::NoRoots(consensus.validate_and_insert_block_batch(blocks)));
                }
            };

        if self.orphans.len() == self.max_orphans {
            let mut eviction_succeeded = false;
            debug!("Orphan blocks pool size exceeded. Trying to evict a random orphan block.");
            // Retry up to a logarithmic number of times
            for i in 0..self.max_orphans_log {
                // Evict a random orphan in order to keep pool size under the limit
                let rand_index = rand::thread_rng().gen_range(0..self.orphans.len());
                if !orphan_ancestors.is_empty() {
                    // IndexMap has no API for getting a removable Entry by index
                    if let Some(rand_hash) = self.orphans.get_index(rand_index).map(|(&h, _)| h)
                        && orphan_ancestors.contains(&rand_hash)
                    {
                        continue; // Do not evict an ancestor of this new orphan
                    }
                }
                if let Some((evicted, _)) = self.orphans.swap_remove_index(rand_index) {
                    debug!("Evicted {} from the orphan blocks pool for new block {} (after {} retries)", evicted, orphan_hash, i);
                    eviction_succeeded = true;
                    break;
                }
            }
            if !eviction_succeeded {
                // All retries have found an existing ancestor, so we reject the new block
                debug!(
                    "Tried to evict a random orphan for new orphan {}, but all {} retries found an existing ancestor. Rejecting.",
                    orphan_hash, self.max_orphans_log
                );
                return None;
            }
        }
        for parent in &block_deps(&orphan_block, self.exec_v2) {
            if let Some(entry) = self.orphans.get_mut(parent) {
                entry.children.insert(orphan_hash);
            }
        }
        // Insert
        self.orphans.insert(orphan_block.hash(), OrphanBlock::new(orphan_block, self.iterate_child_orphans(orphan_hash).collect()));
        // Return roots
        Some(OrphanOutput::Roots(roots))
    }

    /// Returns whether this block is in the orphan pool.
    pub fn is_known_orphan(&self, hash: BlockHash) -> bool {
        self.orphans.contains_key(&hash)
    }

    /// Returns the orphan roots of the provided orphan. Orphan roots are ancestors of this orphan which are
    /// not in the orphan pool AND do not exist consensus-wise or are header-only. Given an orphan relayed by
    /// a peer, these blocks should be the next-in-line to be requested from that peer.
    pub async fn get_orphan_roots_if_known(&self, consensus: &ConsensusProxy, orphan: BlockHash) -> OrphanOutput {
        if let Some(orphan_block) = self.orphans.get(&orphan) {
            match self.get_orphan_roots(consensus, block_deps(&orphan_block.block, self.exec_v2).into_iter().collect()).await {
                FindRootsOutput::Roots(roots, _) => OrphanOutput::Roots(roots),
                FindRootsOutput::NoRoots(_) => OrphanOutput::NoRoots(Default::default()),
            }
        } else {
            OrphanOutput::Unknown
        }
    }

    /// Internal get roots method. The arg `queue` is the set of blocks to perform BFS from and
    /// search through the orphan pool and consensus until finding any unknown roots or finding
    /// out that no ancestor is missing.
    async fn get_orphan_roots(&self, consensus: &ConsensusProxy, mut queue: VecDeque<BlockHash>) -> FindRootsOutput {
        let mut roots = Vec::new();
        let mut visited: HashSet<_> = queue.iter().copied().collect();
        let mut orphan_ancestors = HashSet::new();
        while let Some(current) = queue.pop_front() {
            if let Some(block) = self.orphans.get(&current) {
                orphan_ancestors.insert(current);
                for parent in block_deps(&block.block, self.exec_v2) {
                    if visited.insert(parent) {
                        queue.push_back(parent);
                    }
                }
            } else {
                let status = consensus.async_get_block_status(current).await;
                if status.is_none_or(|s| s.is_header_only()) {
                    // Block is not in the orphan pool nor does its body exist consensus-wise, so it is a root
                    roots.push(current);
                }
            }
        }

        if roots.is_empty() { FindRootsOutput::NoRoots(orphan_ancestors) } else { FindRootsOutput::Roots(roots, orphan_ancestors) }
    }

    pub async fn unorphan_blocks(
        &mut self,
        consensus: &ConsensusProxy,
        root: BlockHash,
    ) -> (Vec<Block>, Vec<BlockValidationFuture>, Vec<BlockValidationFuture>) {
        let root_entry = self.orphans.swap_remove(&root); // Try removing the root just in case it was previously an orphan
        let mut process_queue =
            ProcessQueue::from(root_entry.map(|e| e.children).unwrap_or_else(|| self.iterate_child_orphans(root).collect()));
        let mut processing = HashMap::new();
        while let Some(orphan_hash) = process_queue.dequeue() {
            if let Occupied(entry) = self.orphans.entry(orphan_hash) {
                let mut processable = true;
                for p in block_deps(&entry.get().block, self.exec_v2) {
                    if !processing.contains_key(&p) && consensus.async_get_block_status(p).await.is_none_or(|s| s.is_header_only()) {
                        processable = false;
                        break;
                    }
                }
                if processable {
                    let orphan_block = entry.swap_remove();
                    let BlockValidationFutures { block_task, virtual_state_task } =
                        consensus.validate_and_insert_block(orphan_block.block.clone());
                    processing.insert(orphan_hash, (orphan_block.block, block_task, virtual_state_task));
                    process_queue.enqueue_chunk(orphan_block.children);
                }
            }
        }
        // We deliberately want all processing tasks to be awaited out of the orphan pool lock
        itertools::multiunzip(processing.into_values())
    }

    fn iterate_child_orphans(&self, hash: BlockHash) -> impl Iterator<Item = BlockHash> + '_ {
        self.orphans.iter().filter_map(move |(&orphan_hash, orphan_block)| {
            if block_deps(&orphan_block.block, self.exec_v2).contains(&hash) { Some(orphan_hash) } else { None }
        })
    }

    /// Iterate all orphans and remove blocks which are no longer orphans.
    /// This is important for the overall health of the pool and for ensuring that
    /// orphan blocks don't evict due to pool size limit while already processed
    /// blocks remain in it. Should be called following IBD.  
    pub async fn revalidate_orphans(&mut self, consensus: &ConsensusProxy) -> (Vec<BlockHash>, Vec<BlockValidationFuture>) {
        // First, cleanup blocks already processed by consensus
        let mut i = 0;
        while i < self.orphans.len() {
            if let Some((&h, _)) = self.orphans.get_index(i) {
                if consensus.async_get_block_status(h).await.is_some_and(|s| s.is_invalid() || s.has_block_body()) {
                    // If we swap removed do not advance i so that we revisit the new element moved
                    // to i in the next iteration. Loop will progress because len is shorter now.
                    self.orphans.swap_remove_index(i);
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        // Next, search for root blocks which are processable. A processable block is a block
        // which all of its parents are known to consensus with valid body state
        let mut roots = Vec::new();
        for block in self.orphans.values() {
            let mut processable = true;
            for parent in block_deps(&block.block, self.exec_v2) {
                if self.orphans.contains_key(&parent)
                    || consensus.async_get_block_status(parent).await.is_none_or(|status| status.is_header_only())
                {
                    processable = false;
                    break;
                }
            }
            if processable {
                roots.push(block.block.clone());
            }
        }

        // Now process the roots and unorphan their descendents
        let mut virtual_processing_tasks = Vec::with_capacity(roots.len());
        let mut queued_hashes = Vec::with_capacity(roots.len());
        for root in roots {
            let root_hash = root.hash();
            // Queue the root for processing
            let BlockValidationFutures { block_task: _, virtual_state_task: root_task } = consensus.validate_and_insert_block(root);
            // Queue its descendents which are processable
            let (descendent_blocks, _, descendents_tasks) = self.unorphan_blocks(consensus, root_hash).await;
            // Keep track of all hashes and tasks
            virtual_processing_tasks.extend(once(root_task).chain(descendents_tasks));
            queued_hashes.extend(once(root_hash).chain(descendent_blocks.into_iter().map(|block| block.hash())));
        }

        // We deliberately want the processing tasks to be awaited out of the orphan pool lock
        (queued_hashes, virtual_processing_tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::try_join_all;
    use kaspa_consensus_core::{
        api::{BlockValidationFutures, ConsensusApi},
        blockstatus::BlockStatus,
        errors::block::BlockProcessResult,
    };
    use kaspa_consensusmanager::{ConsensusInstance, SessionLock};
    use kaspa_core::assert_match;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[derive(Default)]
    struct MockProcessor {
        processed: Arc<RwLock<HashSet<BlockHash>>>,
    }

    async fn block_process_mock() -> BlockProcessResult<BlockStatus> {
        Ok(BlockStatus::StatusUTXOPendingVerification)
    }

    impl ConsensusApi for MockProcessor {
        fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
            self.processed.write().insert(block.hash());
            BlockValidationFutures { block_task: Box::pin(block_process_mock()), virtual_state_task: Box::pin(block_process_mock()) }
        }

        fn get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
            self.processed.read().get(&hash).map(|_| BlockStatus::StatusUTXOPendingVerification)
        }
    }

    #[tokio::test]
    async fn test_orphan_pool_basics() {
        let max_orphans = 10;
        let ci = ConsensusInstance::new(SessionLock::new(), Arc::new(MockProcessor::default()));
        let consensus = ci.session().await;
        let mut pool = OrphanBlocksPool::new(max_orphans);

        let roots = vec![8.into(), 9.into()];
        let a = Block::from_precomputed_hash(8.into(), vec![]);
        let b = Block::from_precomputed_hash(9.into(), vec![]);
        let c = Block::from_precomputed_hash(10.into(), roots.clone());
        let d = Block::from_precomputed_hash(11.into(), vec![10.into()]);

        let e = Block::from_precomputed_hash(12.into(), vec![10.into()]);
        let f = Block::from_precomputed_hash(13.into(), vec![12.into()]);
        let g = Block::from_precomputed_hash(14.into(), vec![13.into()]);
        let h = Block::from_precomputed_hash(15.into(), vec![14.into()]);
        let k = Block::from_precomputed_hash(16.into(), vec![15.into()]);

        pool.add_orphan(&consensus, c.clone()).await.unwrap();
        pool.add_orphan(&consensus, d.clone()).await.unwrap();

        assert_match!(pool.get_orphan_roots_if_known(&consensus, d.hash()).await, OrphanOutput::Roots(recv_roots) if recv_roots == roots);

        consensus.validate_and_insert_block(a.clone()).virtual_state_task.await.unwrap();
        consensus.validate_and_insert_block(b.clone()).virtual_state_task.await.unwrap();

        // Test unorphaning
        let (blocks, _, virtual_state_tasks) = pool.unorphan_blocks(&consensus, 8.into()).await;
        try_join_all(virtual_state_tasks).await.unwrap();
        assert_eq!(blocks.into_iter().map(|b| b.hash()).collect::<HashSet<_>>(), HashSet::from([10.into(), 11.into()]));
        assert!(pool.orphans.is_empty());

        // Test revalidation
        pool.add_orphan(&consensus, f.clone()).await.unwrap();
        pool.add_orphan(&consensus, g.clone()).await.unwrap();
        pool.add_orphan(&consensus, k.clone()).await.unwrap();
        assert_eq!(pool.orphans.len(), 3);
        consensus.validate_and_insert_block(e.clone()).virtual_state_task.await.unwrap();
        pool.revalidate_orphans(&consensus).await;
        assert_eq!(pool.orphans.len(), 1);
        assert!(pool.orphans.contains_key(&k.hash())); // k's parent, h, was never inserted to the pool
        consensus.validate_and_insert_block(h.clone()).virtual_state_task.await.unwrap();
        pool.revalidate_orphans(&consensus).await;
        assert!(pool.orphans.is_empty());

        drop((a, b, c, d, e, f, g, h, k));
    }

    /// A pipeline that refuses a block handed over before one of its parents — what the real one
    /// does when that parent is not even in flight yet.
    #[derive(Default)]
    struct ParentCheckingProcessor {
        processed: Arc<RwLock<Vec<BlockHash>>>,
    }

    impl ConsensusApi for ParentCheckingProcessor {
        fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
            let mut processed = self.processed.write();
            let missing: Vec<BlockHash> =
                block.header.direct_parents().iter().copied().filter(|parent| !processed.contains(parent)).collect();
            let result: BlockProcessResult<BlockStatus> = if missing.is_empty() {
                processed.push(block.hash());
                Ok(BlockStatus::StatusUTXOPendingVerification)
            } else {
                Err(kaspa_consensus_core::errors::block::RuleError::MissingParents(missing))
            };
            BlockValidationFutures {
                block_task: Box::pin(std::future::ready(result.clone())),
                virtual_state_task: Box::pin(std::future::ready(result)),
            }
        }

        fn get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
            self.processed.read().contains(&hash).then_some(BlockStatus::StatusUTXOPendingVerification)
        }
    }

    /// **A round lane in the orphan pool is handed to consensus parents-first.** A lane's blocks tie
    /// on blue work (ADR-0125) — here every block carries the default, as a lane's all carry one —
    /// so the batch's blue-work sort leaves them in the pool's hash-set order, and a child ahead of
    /// its parent is refused with `MissingParents`. The batch now puts parents first.
    #[tokio::test]
    async fn an_orphan_lane_that_ties_on_blue_work_is_handed_over_parents_first() {
        let processor = ParentCheckingProcessor::default();
        let processed = processor.processed.clone();
        let ci = ConsensusInstance::new(SessionLock::new(), Arc::new(processor));
        let consensus = ci.session().await;
        let mut pool = OrphanBlocksPool::new(16);

        // The lane 2 ← 3 ← 4 ← 5 ← 6 hangs from 1, and arrives before 1 does, tip first.
        let anchor = Block::from_precomputed_hash(1.into(), vec![]);
        for i in (2u64..=6).rev() {
            let block = Block::from_precomputed_hash(i.into(), vec![(i - 1).into()]);
            assert_match!(pool.add_orphan(&consensus, block).await, Some(OrphanOutput::Roots(_)));
        }
        consensus.validate_and_insert_block(anchor).virtual_state_task.await.unwrap();

        // A block on the lane's tip: every ancestor is known or in the pool, so the pool hands the
        // whole lane over as one batch.
        let tip = Block::from_precomputed_hash(7.into(), vec![6.into()]);
        let Some(OrphanOutput::NoRoots(batch)) = pool.add_orphan(&consensus, tip).await else {
            panic!("every ancestor is in the pool or known");
        };
        let order: Vec<BlockHash> = batch.blocks.iter().map(|block| block.hash()).collect();
        assert_eq!(order, (2u64..=6).map(BlockHash::from).collect::<Vec<_>>(), "parents first, whatever the hash set's order");
        try_join_all(batch.virtual_state_tasks.unwrap()).await.expect("every lane block lands behind its parent");
        assert_eq!(*processed.read(), (1u64..=6).map(BlockHash::from).collect::<Vec<_>>());
    }

    /// The EXEC payload in force from genesis, as the anchoring tests below run it.
    const ARMED: Option<ForkActivation> = Some(ForkActivation::always());

    /// A pipeline that, like the real body stage, refuses a block whose lane heads (RFC-0008 v2) are not in yet, as it does a block whose
    /// parent is not.
    #[derive(Default)]
    struct DependencyCheckingProcessor {
        processed: Arc<RwLock<Vec<BlockHash>>>,
    }

    impl ConsensusApi for DependencyCheckingProcessor {
        fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
            let mut processed = self.processed.write();
            let missing: Vec<BlockHash> = block_deps(&block, ARMED).into_iter().filter(|dep| !processed.contains(dep)).collect();
            let result: BlockProcessResult<BlockStatus> = if missing.is_empty() {
                processed.push(block.hash());
                Ok(BlockStatus::StatusUTXOPendingVerification)
            } else {
                Err(kaspa_consensus_core::errors::block::RuleError::MissingParents(missing))
            };
            BlockValidationFutures {
                block_task: Box::pin(std::future::ready(result.clone())),
                virtual_state_task: Box::pin(std::future::ready(result)),
            }
        }

        fn get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
            self.processed.read().contains(&hash).then_some(BlockStatus::StatusUTXOPendingVerification)
        }
    }

    /// A chain block `hash` hung from `parent` whose coinbase anchors the lane heads `heads` (ascending), which it does NOT name as parents.
    fn anchoring_block(hash: u64, parent: u64, heads: &[u64]) -> Block {
        use kaspa_consensus_core::palw_exec_v2_anchor::{PalwExecV2AnchorV1, palw_exec_v2_anchor_append};
        let anchor = PalwExecV2AnchorV1 {
            heads: heads.iter().map(|head| BlockHash::from(*head)).collect(),
            count: heads.len() as u32,
            root: kaspa_hashes::Hash64::from_u64_word(7),
        };
        let payload = palw_exec_v2_anchor_append(&[0u8; 10], &anchor).expect("a well-formed trailer");
        let coinbase = kaspa_consensus_core::tx::Transaction::new(
            0,
            Vec::new(),
            Vec::new(),
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE,
            0,
            payload,
        );
        let mut block = Block::from_precomputed_hash(hash.into(), vec![parent.into()]);
        block.transactions = Arc::new(vec![coinbase]);
        block
    }

    /// **An anchoring block waits for the lane heads it names** (RFC-0008 v2) exactly as it waits for a parent. The heads are no parent of
    /// anything, so a pool that looked only at direct parents would hand the block to consensus the moment its parent landed — and
    /// consensus would refuse it, `MissingParents`, with nothing to retry it — or, past a restart, lose the lane for good. Here the
    /// block is an orphan whose roots are the missing heads (so the node requests them), it stays an orphan while ONE head is in, and
    /// it is released the moment the last arrives.
    #[tokio::test]
    async fn an_anchoring_block_waits_for_its_lane_heads_as_for_its_parents() {
        let processor = DependencyCheckingProcessor::default();
        let processed = processor.processed.clone();
        let ci = ConsensusInstance::new(SessionLock::new(), Arc::new(processor));
        let consensus = ci.session().await;
        let mut pool = OrphanBlocksPool::new(16).with_exec_v2(ARMED);

        // The chain block 1 is known; the lane 20 <- 21 hangs from it; block 30 builds on 1 and anchors the lane's head set {20, 21}.
        let chain = Block::from_precomputed_hash(1.into(), vec![]);
        let head_a = Block::from_precomputed_hash(20.into(), vec![1.into()]);
        let head_b = Block::from_precomputed_hash(21.into(), vec![1.into()]);
        let anchoring = anchoring_block(30, 1, &[20, 21]);
        assert_eq!(block_deps(&anchoring, ARMED), vec![BlockHash::from(1u64), 20.into(), 21.into()], "parents, then the heads");
        consensus.validate_and_insert_block(chain).virtual_state_task.await.unwrap();

        // Its parent is known, its heads are not: an orphan, and the pool names the heads as what to request.
        let Some(OrphanOutput::Roots(roots)) = pool.add_orphan(&consensus, anchoring.clone()).await else {
            panic!("the anchoring block is an orphan until its heads are known");
        };
        let roots: HashSet<BlockHash> = roots.into_iter().collect();
        assert_eq!(roots, HashSet::from([20.into(), 21.into()]), "the missing heads are the roots");
        assert!(pool.orphans.contains_key(&anchoring.hash()));

        // One head in: still waiting for the other.
        consensus.validate_and_insert_block(head_a).virtual_state_task.await.unwrap();
        let (blocks, _, _) = pool.unorphan_blocks(&consensus, 20.into()).await;
        assert!(blocks.is_empty(), "one head is not both");
        assert!(pool.orphans.contains_key(&anchoring.hash()));

        // The second head: the block is released, and consensus takes it.
        consensus.validate_and_insert_block(head_b).virtual_state_task.await.unwrap();
        let (blocks, _, tasks) = pool.unorphan_blocks(&consensus, 21.into()).await;
        assert_eq!(blocks.iter().map(|block| block.hash()).collect::<Vec<_>>(), vec![anchoring.hash()]);
        try_join_all(tasks).await.expect("with its heads in, the anchoring block lands");
        assert!(pool.orphans.is_empty());
        assert!(processed.read().contains(&anchoring.hash()));

        // A block with no trailer names no head: its dependencies are its parents alone.
        assert_eq!(block_deps(&Block::from_precomputed_hash(40.into(), vec![1.into()]), ARMED), vec![BlockHash::from(1u64)]);
    }

    /// **Fence off, a coinbase is the miner's bytes** (the X8R review). A block whose tag ends in a well-formed trailer naming hashes
    /// nobody has is, where `palw_exec_payload_v2` is not armed — or armed above the block's DAA — an orphan of its PARENT alone: the
    /// pool names only the parent as a root and releases the block the moment the parent lands, exactly as a pool that never heard of
    /// RFC-0008 v2 does. (With the read unconditional, the same block waited for the named hashes forever on an upgraded node.)
    #[tokio::test]
    async fn fence_off_a_trailer_in_a_miners_tag_is_not_a_dependency() {
        let anchoring = anchoring_block(30, 1, &[20, 21]);
        for exec_v2 in [None, Some(ForkActivation::new(1_000))] {
            assert_eq!(block_deps(&anchoring, exec_v2), vec![BlockHash::from(1u64)], "{exec_v2:?}: the parents alone");

            let processor = ParentCheckingProcessor::default();
            let processed = processor.processed.clone();
            let ci = ConsensusInstance::new(SessionLock::new(), Arc::new(processor));
            let consensus = ci.session().await;
            let mut pool = OrphanBlocksPool::new(16).with_exec_v2(exec_v2);
            let Some(OrphanOutput::Roots(roots)) = pool.add_orphan(&consensus, anchoring.clone()).await else {
                panic!("an orphan of its missing parent");
            };
            assert_eq!(roots, vec![BlockHash::from(1u64)], "the parent is the only root: the trailer's hashes are never requested");
            consensus.validate_and_insert_block(Block::from_precomputed_hash(1.into(), vec![])).virtual_state_task.await.unwrap();
            let (blocks, _, tasks) = pool.unorphan_blocks(&consensus, 1.into()).await;
            assert_eq!(
                blocks.iter().map(|block| block.hash()).collect::<Vec<_>>(),
                vec![anchoring.hash()],
                "released with its parent"
            );
            try_join_all(tasks).await.expect("and consensus takes it");
            assert!(pool.orphans.is_empty());
            assert!(processed.read().contains(&anchoring.hash()));
        }
    }
}
