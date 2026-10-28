//! **IBD headers go to consensus parents-first, whatever order the syncer sent them in.**
//!
//! The pipeline takes a header whose parent is still being processed — it waits for it — but not
//! one whose parent it has never been given: that is `MissingParents` on the spot, and the IBD round
//! fails and drops the peer. Upstream never meets that, because a syncer sends every batch in
//! blue-work order and there blue work is a topological order. On a round-lane network it is not
//! (ADR-0125: a lane's blocks all carry their anchor's weight and tie, and ties go by hash), and the
//! fleet already running sends lanes child-first. testnet-12 from DAA 316: no fresh node, and no
//! node that fell back into IBD across the first lane, could finish.
//!
//! So a syncing node orders what it receives itself: each chunk, with whatever it is still holding,
//! is put parents-first, and a header whose parent has neither arrived nor is known to consensus is
//! HELD until the chunk carrying that parent comes, instead of being handed over to fail. A chunk
//! that is already parents-first and whose parents are all known — every chunk upstream ever sends
//! — is handed over exactly as received.
//!
//! A syncer that never sends the parent gains nothing by it: what is still held when the stream
//! ends is handed over anyway, and consensus refuses it with the same `MissingParents` it would have
//! refused it with before; and the buffer is bounded, past which it stops waiting in the same way.

use kaspa_consensus_core::{BlockHash, BlockHashSet, HashMapCustomHasher, header::Header, topological_order::release_parent_first};
use kaspa_consensusmanager::ConsensusProxy;
use std::{collections::VecDeque, sync::Arc};

/// How many headers may wait for a parent before the feeder stops waiting. Legitimate waiting spans
/// one mergeset split across two chunks; a syncer's chunk is at most 1,024 headers (`RequestHeaders`).
/// Beyond this the held headers are handed over, and consensus reports their missing parent.
pub(crate) const MAX_HELD_HEADERS: usize = 4 * 1024;

/// How many previous hand-overs may still be in the pipeline when the next chunk is ordered. The
/// header sync joins chunk `k - 1` only after handing over chunk `k`, so a parent handed over in
/// either of the last two may not be visible in the status store yet.
const IN_FLIGHT_HANDOVERS: usize = 2;

pub(crate) struct ParentFirstHeaders {
    /// Headers waiting for a parent that has not arrived, parents-first.
    held: Vec<Arc<Header>>,
    /// The hashes of the last [`IN_FLIGHT_HANDOVERS`] hand-overs, newest last.
    handed_over: VecDeque<BlockHashSet>,
}

impl ParentFirstHeaders {
    pub(crate) fn new() -> Self {
        Self { held: Vec::new(), handed_over: VecDeque::with_capacity(IN_FLIGHT_HANDOVERS) }
    }

    /// Orders `chunk` — behind anything still held — parents-first, and returns the headers to hand
    /// to consensus now, in the order to hand them over. Everything else is held for a later chunk.
    pub(crate) async fn admit(&mut self, consensus: &ConsensusProxy, chunk: Vec<Arc<Header>>) -> Vec<Arc<Header>> {
        let mut batch = std::mem::take(&mut self.held);
        batch.extend(chunk);
        // Parents neither in this batch nor handed over lately: only consensus can say whether it
        // holds them. One lookup for the lot — on a well-ordered stream this is the first chunk's
        // parents and little else.
        let in_batch: BlockHashSet = batch.iter().map(|header| header.hash).collect();
        let mut asked = BlockHashSet::new();
        let unknown: Vec<BlockHash> = batch
            .iter()
            .flat_map(|header| header.direct_parents().iter().copied())
            .filter(|parent| !in_batch.contains(parent) && !self.was_handed_over(parent) && asked.insert(*parent))
            .collect();
        let known: BlockHashSet = if unknown.is_empty() {
            BlockHashSet::new()
        } else {
            consensus
                .clone()
                .spawn_blocking(move |c| unknown.into_iter().filter(|hash| c.get_block_status(*hash).is_some()).collect())
                .await
        };
        let (released, held) = release_parent_first(batch, |parent| known.contains(&parent) || self.was_handed_over(&parent));
        self.hand_over(released, held)
    }

    /// Whatever is still held: the stream has ended, so the parents it waits for are not coming.
    /// Hand it over anyway and let consensus say so (`MissingParents`), as it would have before.
    pub(crate) fn drain(&mut self) -> Vec<Arc<Header>> {
        std::mem::take(&mut self.held)
    }

    fn hand_over(&mut self, mut released: Vec<Arc<Header>>, held: Vec<Arc<Header>>) -> Vec<Arc<Header>> {
        if held.len() > MAX_HELD_HEADERS {
            // Waiting this long is not a split mergeset: stop, and let consensus refuse them now.
            released.extend(held);
        } else {
            self.held = held;
        }
        if self.handed_over.len() == IN_FLIGHT_HANDOVERS {
            self.handed_over.pop_front();
        }
        self.handed_over.push_back(released.iter().map(|header| header.hash).collect());
        released
    }

    fn was_handed_over(&self, hash: &BlockHash) -> bool {
        self.handed_over.iter().any(|set| set.contains(hash))
    }

    #[cfg(test)]
    pub(crate) fn held_len(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::try_join_all;
    use kaspa_consensus_core::{
        api::{BlockValidationFutures, ConsensusApi},
        block::Block,
        blockstatus::BlockStatus,
        errors::block::{BlockProcessResult, RuleError},
    };
    use kaspa_consensusmanager::{ConsensusInstance, SessionLock};
    use parking_lot::RwLock;

    /// A pipeline with the one rule this is about: a header is taken only once every parent has been
    /// taken before it (the real pipeline also waits on a parent still in flight; this one is
    /// synchronous, so "taken before" is the same thing).
    #[derive(Default)]
    struct ParentCheckingPipeline {
        taken: Arc<RwLock<Vec<BlockHash>>>,
    }

    impl ConsensusApi for ParentCheckingPipeline {
        fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
            let mut taken = self.taken.write();
            let missing: Vec<BlockHash> =
                block.header.direct_parents().iter().copied().filter(|parent| !taken.contains(parent)).collect();
            let result: BlockProcessResult<BlockStatus> = if missing.is_empty() {
                taken.push(block.hash());
                Ok(BlockStatus::StatusHeaderOnly)
            } else {
                Err(RuleError::MissingParents(missing))
            };
            let block_task = Box::pin(std::future::ready(result.clone()));
            let virtual_state_task = Box::pin(std::future::ready(result));
            BlockValidationFutures { block_task, virtual_state_task }
        }

        fn get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
            self.taken.read().contains(&hash).then_some(BlockStatus::StatusHeaderOnly)
        }
    }

    fn h(word: u64) -> BlockHash {
        BlockHash::from_u64_word(word)
    }

    fn header(hash: u64, parents: &[u64]) -> Arc<Header> {
        Arc::new(Header::from_precomputed_hash(h(hash), parents.iter().copied().map(h).collect()))
    }

    fn pipeline_with_genesis() -> (ConsensusProxy, Arc<RwLock<Vec<BlockHash>>>) {
        let pipeline = ParentCheckingPipeline::default();
        pipeline.taken.write().push(h(0));
        let taken = pipeline.taken.clone();
        let instance = ConsensusInstance::new(SessionLock::new(), Arc::new(pipeline));
        (instance.unguarded_session(), taken)
    }

    async fn hand(consensus: &ConsensusProxy, headers: Vec<Arc<Header>>) -> BlockProcessResult<Vec<BlockStatus>> {
        try_join_all(
            headers.into_iter().map(|header| consensus.validate_and_insert_block(Block::from_header_arc(header)).virtual_state_task),
        )
        .await
    }

    /// The testnet-12 lane as an un-upgraded syncer sends it: chain block 1, then its lane
    /// `r2 ← r3 ← r4` in hash order (4, 3, 2) — and, in the next chunk, a chain block 5 merging the
    /// lane's tip. Handed over as received, the first child fails; through the feeder, all land.
    fn chunks_as_the_fleet_sends_them() -> Vec<Vec<Arc<Header>>> {
        vec![vec![header(1, &[0]), header(4, &[3]), header(3, &[2]), header(2, &[1])], vec![header(5, &[1, 4])]]
    }

    #[tokio::test]
    async fn a_lane_sent_child_first_fails_as_received_and_lands_through_the_feeder() {
        let (consensus, _) = pipeline_with_genesis();
        let as_received = chunks_as_the_fleet_sends_them().concat();
        match hand(&consensus, as_received).await {
            Err(RuleError::MissingParents(missing)) => assert_eq!(missing, vec![h(3)], "round block 4 went ahead of its parent"),
            other => panic!("expected the IBD round to fail as it does on testnet-12, got {other:?}"),
        }

        let (consensus, taken) = pipeline_with_genesis();
        let mut feeder = ParentFirstHeaders::new();
        for chunk in chunks_as_the_fleet_sends_them() {
            let released = feeder.admit(&consensus, chunk).await;
            hand(&consensus, released).await.expect("every header lands behind its parents");
        }
        assert!(feeder.drain().is_empty());
        assert_eq!(*taken.read(), vec![h(0), h(1), h(2), h(3), h(4), h(5)]);
    }

    /// A mergeset split across two chunks with the parent in the SECOND: the child waits one chunk.
    #[tokio::test]
    async fn a_header_whose_parent_is_in_the_next_chunk_waits_for_it() {
        let (consensus, taken) = pipeline_with_genesis();
        let mut feeder = ParentFirstHeaders::new();
        let released = feeder.admit(&consensus, vec![header(1, &[0]), header(3, &[2])]).await;
        assert_eq!(released.iter().map(|x| x.hash).collect::<Vec<_>>(), vec![h(1)]);
        assert_eq!(feeder.held_len(), 1);
        hand(&consensus, released).await.unwrap();
        let released = feeder.admit(&consensus, vec![header(2, &[1]), header(6, &[3])]).await;
        assert_eq!(released.iter().map(|x| x.hash).collect::<Vec<_>>(), vec![h(2), h(3), h(6)]);
        hand(&consensus, released).await.unwrap();
        assert_eq!(feeder.held_len(), 0);
        assert_eq!(*taken.read(), vec![h(0), h(1), h(2), h(3), h(6)]);
    }

    /// A chunk upstream would send — parents first, every parent known — is handed over as received,
    /// the same allocations in the same order.
    #[tokio::test]
    async fn a_well_ordered_chunk_is_handed_over_as_received() {
        let (consensus, _) = pipeline_with_genesis();
        let mut feeder = ParentFirstHeaders::new();
        let chunk = vec![header(1, &[0]), header(2, &[1]), header(7, &[1]), header(3, &[2, 7])];
        let released = feeder.admit(&consensus, chunk.clone()).await;
        assert_eq!(released.len(), chunk.len());
        assert!(released.iter().zip(chunk.iter()).all(|(a, b)| Arc::ptr_eq(a, b)));
    }

    /// A parent that never comes: nothing waits forever, and consensus gets to refuse the orphan with
    /// the error it always did.
    #[tokio::test]
    async fn a_parent_that_never_comes_is_still_refused_by_consensus() {
        let (consensus, _) = pipeline_with_genesis();
        let mut feeder = ParentFirstHeaders::new();
        let released = feeder.admit(&consensus, vec![header(1, &[0]), header(9, &[8])]).await;
        hand(&consensus, released).await.unwrap();
        let rest = feeder.drain();
        assert_eq!(rest.len(), 1);
        assert!(matches!(hand(&consensus, rest).await, Err(RuleError::MissingParents(_))));
    }

    /// The buffer is bounded: past [`MAX_HELD_HEADERS`] the feeder stops waiting and hands them over.
    #[tokio::test]
    async fn the_feeder_does_not_hold_without_bound() {
        let (consensus, _) = pipeline_with_genesis();
        let mut feeder = ParentFirstHeaders::new();
        let orphans: Vec<Arc<Header>> = (0..=MAX_HELD_HEADERS as u64).map(|i| header(1_000 + i, &[999])).collect();
        let released = feeder.admit(&consensus, orphans).await;
        assert_eq!(released.len(), MAX_HELD_HEADERS + 1);
        assert_eq!(feeder.held_len(), 0);
    }
}
