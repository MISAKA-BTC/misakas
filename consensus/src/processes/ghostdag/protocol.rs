use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashMap, BlockLevel, BlueWorkType, HashMapCustomHasher,
    blockhash::{self, BlockHashExtensions, BlockHashes},
};
use kaspa_utils::refs::Refs;

use crate::{
    model::{
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{GhostdagData, GhostdagStoreReader, HashKTypeMap, KType},
            headers::HeaderStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::difficulty::{calc_work, level_work},
};

use super::ordering::*;

#[derive(Clone)]
pub struct GhostdagManager<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> {
    genesis_hash: BlockHash,
    pub(super) k: KType,
    pub(super) ghostdag_store: Arc<T>,
    pub(super) relations_store: S,
    pub(super) headers_store: Arc<V>,
    pub(super) reachability_service: U,

    /// Level work is a lower-bound for the amount of work represented by each block.
    /// When running GD for higher-level sub-DAGs, this value should be set accordingly
    /// to the work represented by that level, and then used as a lower bound
    /// for the work calculated from header bits (which depends on current difficulty).
    /// For instance, assuming level 80 (i.e., pow hash has at least 80 zeros) is always
    /// above the difficulty target, all blocks in it should represent the same amount of
    /// work regardless of whether current difficulty requires 20 zeros or 25 zeros.
    level_work: BlueWorkType,

    /// ADR-0060 Decision 1.2: on a `ConsensusV2` network a heartbeat (algo-3) block's blue work
    /// is the fixed ε — at EVERY level, unmaxed with `level_work`, or an ASIC pointed at the
    /// lane would buy pruning-proof weight instead of chain weight. False on every non-V2
    /// network, where algo-3 is the real production lane and its bits are the real price.
    /// ADR-0066: the heartbeat lane's fence, mode folded in (`Params::palw_heartbeat_lane_fence`).
    heartbeat_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,

    /// ADR-0066 Decision 3 (finding F2), closed by ADR-0068 Phase 1: under this fence an
    /// attempt-lane (algo-6) block's blue work is the constant
    /// `1 << PALW_ATTEMPT_BLUE_WORK_LOG2` instead of `calc_work(bits)` — on a V2 network the
    /// bits sit at the ambient maximum and price every bonded block at 2, parity with two ε = 1
    /// heartbeats. NOT maxed with `level_work`, for the same reason ε is not: the attempt lane is the real production
    /// lane and its inference-priced digest is what the pruning-proof hierarchy is built from.
    /// Mode folded in (`Params::palw_attempt_work_fence`).
    attempt_work_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> GhostdagManager<T, S, U, V> {
    pub fn new(
        genesis_hash: BlockHash,
        k: KType,
        ghostdag_store: Arc<T>,
        relations_store: S,
        headers_store: Arc<V>,
        reachability_service: U,
        // ADR-0060: true exactly on `ConsensusV2` networks — heartbeat blocks then weigh ε.
        heartbeat_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
        // ADR-0068 Phase 1 (F2): attempt-lane blocks then weigh the network constant.
        attempt_work_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
    ) -> Self {
        // For ordinary GD, always keep level_work=0 so the lower bound is ineffective
        Self {
            genesis_hash,
            k,
            ghostdag_store,
            relations_store,
            reachability_service,
            headers_store,
            level_work: 0.into(),
            heartbeat_lane,
            attempt_work_lane,
        }
    }

    pub fn with_level(
        genesis_hash: BlockHash,
        k: KType,
        ghostdag_store: Arc<T>,
        relations_store: S,
        headers_store: Arc<V>,
        reachability_service: U,
        level: BlockLevel,
        max_block_level: BlockLevel,
        // ADR-0060: the ε rule holds at every level — see `heartbeat_lane` on the struct.
        heartbeat_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
        // ADR-0068 Phase 1 (F2): and so does the attempt constant — see the struct field.
        attempt_work_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
    ) -> Self {
        Self {
            genesis_hash,
            k,
            ghostdag_store,
            relations_store,
            reachability_service,
            headers_store,
            level_work: level_work(level, max_block_level),
            heartbeat_lane,
            attempt_work_lane,
        }
    }

    pub fn genesis_ghostdag_data(&self) -> GhostdagData {
        GhostdagData::new(
            0,
            Default::default(),
            blockhash::ORIGIN,
            BlockHashes::new(Vec::new()),
            BlockHashes::new(Vec::new()),
            HashKTypeMap::new(BlockHashMap::new()),
        )
    }

    pub fn origin_ghostdag_data(&self) -> Arc<GhostdagData> {
        Arc::new(GhostdagData::new(
            0,
            Default::default(),
            0.into(),
            BlockHashes::new(Vec::new()),
            BlockHashes::new(Vec::new()),
            HashKTypeMap::new(BlockHashMap::new()),
        ))
    }

    pub fn find_selected_parent(&self, parents: impl IntoIterator<Item = BlockHash>) -> BlockHash {
        parents
            .into_iter()
            .map(|parent| SortableBlock { hash: parent, blue_work: self.ghostdag_store.get_blue_work(parent).unwrap() })
            .max()
            .unwrap()
            .hash
    }

    /// Runs the GHOSTDAG protocol and calculates the block GhostdagData by the given parents.
    /// The function calculates mergeset blues by iterating over the blocks in
    /// the anticone of the new block selected parent (which is the parent with the
    /// highest blue work) and adds any block to the blue set if by adding
    /// it these conditions will not be violated:
    ///
    /// 1) |anticone-of-candidate-block ∩ blue-set-of-new-block| ≤ K
    ///
    /// 2) For every blue block in blue-set-of-new-block:
    ///    |(anticone-of-blue-block ∩ blue-set-new-block) ∪ {candidate-block}| ≤ K.
    ///    We validate this condition by maintaining a map blues_anticone_sizes for
    ///    each block which holds all the blue anticone sizes that were affected by
    ///    the new added blue blocks.
    ///    So to find out what is |anticone-of-blue ∩ blue-set-of-new-block| we just iterate in
    ///    the selected parent chain of the new block until we find an existing entry in
    ///    blues_anticone_sizes.
    ///
    /// For further details see the article <https://eprint.iacr.org/2018/104.pdf>
    pub fn ghostdag(&self, parents: &[BlockHash]) -> GhostdagData {
        assert!(!parents.is_empty(), "genesis must be added via a call to init");

        // Run the GHOSTDAG parent selection algorithm
        let selected_parent = self.find_selected_parent(parents.iter().copied());
        // Handle the special case of origin children first
        if selected_parent.is_origin() {
            // ORIGIN is always a single parent so both blue score and work should remain zero
            return GhostdagData::new_with_selected_parent(selected_parent, 1); // k is only a capacity hint here
        }
        let k = self.k;
        // Initialize new GHOSTDAG block data with the selected parent
        let mut new_block_data = GhostdagData::new_with_selected_parent(selected_parent, k);
        // Get the mergeset in consensus-agreed topological order (topological here means forward in time from blocks to children)
        let ordered_mergeset = self.ordered_mergeset_without_selected_parent(selected_parent, parents);

        for blue_candidate in ordered_mergeset.iter().cloned() {
            let coloring = self.check_blue_candidate(&new_block_data, blue_candidate, k);

            if let ColoringOutput::Blue(blue_anticone_size, blues_anticone_sizes) = coloring {
                // No k-cluster violation found, we can now set the candidate block as blue
                new_block_data.add_blue(blue_candidate, blue_anticone_size, &blues_anticone_sizes);
            } else {
                new_block_data.add_red(blue_candidate);
            }
        }

        let blue_score = self.ghostdag_store.get_blue_score(selected_parent).unwrap() + new_block_data.mergeset_blues.len() as u64;

        let added_blue_work: BlueWorkType = new_block_data
            .mergeset_blues
            .iter()
            .cloned()
            .map(|hash| {
                let header = self.headers_store.get_header(hash).unwrap();
                // **A receipt block's work is zero** (ADR-0044 Decision 6, extended).
                //
                // `calc_block_level_check_pow_layer0` already refuses to derive a block LEVEL from
                // a receipt digest, because nothing in a receipt header costs anything to re-roll
                // and hierarchy position would be sold for the price of one signature. Chain
                // WEIGHT is the same purchase and a cheaper one: blue work is what decides which
                // chain wins, so a receipt block that adds `calc_work(bits)` lets a producer mint
                // reorg weight out of signatures.
                //
                // The lane's meter is the quantum ticket, and the ticket is chain-relative by
                // construction — it draws against a beacon derived from the candidate's own chain,
                // which is exactly why it can only run on a chain candidate and cannot gate DAG
                // entry. So a merged-but-never-candidate receipt block never faces it. Zero is the
                // only figure that is right whether or not the ticket ever runs: all chain weight
                // comes from the attempt lane, whose digests are inference-priced.
                if kaspa_consensus_core::pow_layer0::algo_id_carries_no_chain_position(header.pow_algo_id) {
                    return BlueWorkType::from(0u64);
                }
                // **A heartbeat block's work is ε** (ADR-0060 Decision 1.2) — the lane sells
                // time, not weight. Fixed and independent of the lane's own difficulty, so an
                // ASIC pointed at it buys cadence-capped, near-weightless blocks; and deliberately
                // NOT `.max(self.level_work)`: a heartbeat digest with many leading zeros earns a
                // hierarchy position (levels are about pruning-proof structure), but at no level
                // may it earn level-sized weight, or the proof comparison would price hash again.
                // One, not zero (the receipt lane's figure above): among heartbeat-only branches —
                // total bonded collapse, the regime the lane exists for — ε × n still orders the
                // longer chain first.
                if header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
                    && self.heartbeat_lane.is_some_and(|fence| fence.is_active(header.daa_score))
                {
                    return BlueWorkType::from(kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_BLUE_WORK_EPSILON);
                }
                // **An attempt block's work is the network constant** (ADR-0066 Decision 3 /
                // finding F2, closed by ADR-0068 Phase 1). On a V2 network `header.bits` sits at
                // the ambient maximum — the class lottery is the throttle, not the hash — so
                // `calc_work` prices every bonded block at 2, parity with two ε = 1 heartbeats
                // for ~280 kH/s. The constant restores the ratio ε was designed around: a bonded
                // block outweighs a million heartbeats. A constant and NOT the envelope's claimed
                // pwu — the claim is only verified against class state on the selected chain, and
                // this function holds only the header; a claim-derived figure would let a
                // shape-valid header that never faces the lottery mint fork-choice weight with a
                // number (see `PALW_ATTEMPT_BLUE_WORK_LOG2`).
                //
                // **And NOT maxed with `level_work`, for the reason ε is not** (mainnet audit).
                // This used to read `.max(self.level_work)`, justified as "this lane's
                // inference-priced digest is what the pruning hierarchy is built from". The digest
                // is not inference-priced in the sense that sentence needs. `level_work` is
                // `1 << (level + 256 - max_level)`, the level comes from the digest's leading
                // zeros, and the digest is GRINDABLE at hash cost alone: `l1_tag_v2` is a free CPU
                // expansion "deliberately, so this stays a nonce search", and the job is computed
                // once per template and reused by every nonce. One inference buys an unbounded
                // nonce search, so leading zeros — and therefore level, and therefore level-sized
                // weight — are bought with hashing.
                //
                // `level_work` is only non-zero under `GhostdagManager::with_level`, whose sole
                // callers are `pruning_proof::build` and `pruning_proof::validate`. So the effect
                // was confined to the comparison a syncing node uses to choose between chains —
                // and confined there it is worse, not better: live fork choice was
                // inference-priced while the proof that decides which history a new node adopts
                // was priced in hashes. Two chains carrying the same inference could be ordered by
                // which one's producers ground harder.
                //
                // The heartbeat arm above already refuses exactly this, in these words: "at no
                // level may it earn level-sized weight, or the proof comparison would price hash
                // again." The rule was right; it was applied to one lane. The lane still DERIVES a
                // level (`algo_id_derives_no_block_level` is false for it) because the hierarchy's
                // STRUCTURE is what levels are for — what it must not derive is weight.
                if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2
                    && self.attempt_work_lane.is_some_and(|fence| fence.is_active(header.daa_score))
                {
                    return BlueWorkType::from(1u64 << kaspa_consensus_core::pow_layer0::PALW_ATTEMPT_BLUE_WORK_LOG2);
                }
                calc_work(header.bits).max(self.level_work)
            })
            .sum();
        let blue_work: BlueWorkType = self.ghostdag_store.get_blue_work(selected_parent).unwrap() + added_blue_work;

        new_block_data.finalize_score_and_work(blue_score, blue_work);

        new_block_data
    }

    fn check_blue_candidate_with_chain_block(
        &self,
        new_block_data: &GhostdagData,
        chain_block: &ChainBlock,
        blue_candidate: BlockHash,
        candidate_blues_anticone_sizes: &mut BlockHashMap<KType>,
        candidate_blue_anticone_size: &mut KType,
        k: KType,
    ) -> ColoringState {
        // If blue_candidate is in the future of chain_block, it means
        // that all remaining blues are in the past of chain_block and thus
        // in the past of blue_candidate. In this case we know for sure that
        // the anticone of blue_candidate will not exceed K, and we can mark
        // it as blue.
        //
        // The new block is always in the future of blue_candidate, so there's
        // no point in checking it.

        // We check if chain_block is not the new block by checking if it has a hash.
        if let Some(hash) = chain_block.hash
            && self.reachability_service.is_dag_ancestor_of(hash, blue_candidate)
        {
            return ColoringState::Blue;
        }

        // Iterate over blue peers and check for k-cluster violations
        for &peer in chain_block.data.mergeset_blues.iter() {
            // Skip blocks that are in the past of blue_candidate (since they are not in its anticone)
            if self.reachability_service.is_dag_ancestor_of(peer, blue_candidate) {
                continue;
            }

            // Otherwise, peer must be in the anticone of blue_candidate, so we check for k limits.
            // Note that peer cannot be in the future of blue_candidate because we process the mergeset
            // in past-to-future topological order, so even if chain_block == new_block, an existing blue
            // cannot be in the future of a candidate blue

            let peer_blue_anticone_size = self.blue_anticone_size(peer, new_block_data);
            candidate_blues_anticone_sizes.insert(peer, peer_blue_anticone_size);

            *candidate_blue_anticone_size += 1;
            if *candidate_blue_anticone_size > k {
                // k-cluster violation: The candidate's blue anticone exceeded k
                return ColoringState::Red;
            }

            if peer_blue_anticone_size == k {
                // k-cluster violation: A block in candidate's blue anticone already
                // has k blue blocks in its own anticone
                return ColoringState::Red;
            }

            // This is a sanity check that validates that a blue
            // block's blue anticone is not already larger than K.
            assert!(peer_blue_anticone_size <= k, "found blue anticone larger than K");
        }

        ColoringState::Pending
    }

    /// Returns the blue anticone size of `block` from the worldview of `context`.
    /// Expects `block` to be in the blue set of `context`
    fn blue_anticone_size(&self, block: BlockHash, context: &GhostdagData) -> KType {
        let mut current_blues_anticone_sizes = HashKTypeMap::clone(&context.blues_anticone_sizes);
        let mut current_selected_parent = context.selected_parent;
        loop {
            if let Some(size) = current_blues_anticone_sizes.get(&block) {
                return *size;
            }

            if current_selected_parent == self.genesis_hash || current_selected_parent == blockhash::ORIGIN {
                panic!("block {block} is not in blue set of the given context");
            }

            current_blues_anticone_sizes = self.ghostdag_store.get_blues_anticone_sizes(current_selected_parent).unwrap();
            current_selected_parent = self.ghostdag_store.get_selected_parent(current_selected_parent).unwrap();
        }
    }

    fn check_blue_candidate(&self, new_block_data: &GhostdagData, blue_candidate: BlockHash, k: KType) -> ColoringOutput {
        // The maximum length of new_block_data.mergeset_blues can be K+1 because
        // it contains the selected parent.
        if new_block_data.mergeset_blues.len() as KType == k + 1 {
            return ColoringOutput::Red;
        }

        let mut candidate_blues_anticone_sizes: BlockHashMap<KType> = BlockHashMap::with_capacity(k as usize);
        // Iterate over all blocks in the blue past of the new block that are not in the past
        // of blue_candidate, and check for each one of them if blue_candidate potentially
        // enlarges their blue anticone to be over K, or that they enlarge the blue anticone
        // of blue_candidate to be over K.
        let mut chain_block = ChainBlock { hash: None, data: new_block_data.into() };
        let mut candidate_blue_anticone_size: KType = 0;

        loop {
            let state = self.check_blue_candidate_with_chain_block(
                new_block_data,
                &chain_block,
                blue_candidate,
                &mut candidate_blues_anticone_sizes,
                &mut candidate_blue_anticone_size,
                k,
            );

            match state {
                ColoringState::Blue => return ColoringOutput::Blue(candidate_blue_anticone_size, candidate_blues_anticone_sizes),
                ColoringState::Red => return ColoringOutput::Red,
                ColoringState::Pending => (), // continue looping
            }

            chain_block = ChainBlock {
                hash: Some(chain_block.data.selected_parent),
                data: self.ghostdag_store.get_data(chain_block.data.selected_parent).unwrap().into(),
            }
        }
    }
}

/// Chain block with attached ghostdag data
struct ChainBlock<'a> {
    hash: Option<BlockHash>, // if set to `None`, signals being the new block
    data: Refs<'a, GhostdagData>,
}

/// Represents the intermediate GHOSTDAG coloring state for the current candidate
enum ColoringState {
    Blue,
    Red,
    Pending,
}

/// Represents the final output of GHOSTDAG coloring for the current candidate
enum ColoringOutput {
    Blue(KType, BlockHashMap<KType>), // (blue anticone size, map of blue anticone sizes for each affected blue)
    Red,
}

#[cfg(test)]
mod lane_weight_tests {
    use crate::processes::difficulty::level_work;
    use kaspa_consensus_core::BlueWorkType;
    use kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_BLUE_WORK_EPSILON;
    use kaspa_consensus_core::pow_layer0::{
        PALW_ATTEMPT_BLUE_WORK_LOG2, POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3,
        algo_id_carries_no_chain_position, algo_id_derives_no_block_level,
    };

    /// **No PALW lane may earn level-sized weight, because levels are bought with hashing.**
    ///
    /// The attempt arm used to read `.max(self.level_work)`. `level_work` is non-zero only under
    /// `GhostdagManager::with_level`, whose only callers are the pruning proof's build and
    /// validate — so the effect sat exactly on the comparison a syncing node uses to choose
    /// between chains, while live fork choice was already a flat constant. Two chains carrying the
    /// same inference could be ordered by which one's producers ground harder.
    ///
    /// **This asserts the removed `.max` was load-bearing rather than decorative**, which is the
    /// part a reader cannot check by looking at the diff: above level 20 it strictly exceeded the
    /// attempt constant, so the grinder was paid.
    #[test]
    fn level_work_outgrows_the_attempt_constant_which_is_why_it_may_not_be_maxed_in() {
        let attempt = BlueWorkType::from(1u64 << PALW_ATTEMPT_BLUE_WORK_LOG2);

        // `level_work` is `1 << (level + 256 - max_block_level)`, so at the shipped ceiling the
        // exponent starts at 31 — and there is no parity point at all. **The very first level a
        // digest can reach already outweighs the attempt constant by 4096x**, and it climbs from
        // there. Whatever the grinder found, the max took it.
        let max: u8 = 225;
        assert_eq!(level_work(0, max), BlueWorkType::from(0u64), "level 0 is the only one the max left alone");
        assert!(level_work(1, max) > attempt, "one level of luck already beat the constant");
        assert_eq!(level_work(1, max), attempt * BlueWorkType::from(4096u64), "…by this much");
        assert!(level_work(8, max) > level_work(1, max), "and it is exponential in the leading zeros");

        // ε was never in that range — the heartbeat arm always refused the max, in these words:
        // "at no level may it earn level-sized weight, or the proof comparison would price hash
        // again". The attempt lane now follows the same rule.
        assert!(BlueWorkType::from(HEARTBEAT_BLUE_WORK_EPSILON) < attempt);

        // What each lane may still DERIVE. Structure, yes — weight, no. The attempt lane keeps its
        // level because the pruning hierarchy is built from it; the receipt lane has neither.
        assert!(!algo_id_derives_no_block_level(POW_ALGO_ID_PALW_COMMITTED_V2), "the attempt lane still places the hierarchy");
        assert!(algo_id_derives_no_block_level(POW_ALGO_ID_HEARTBEAT_V1), "a fixed target buys no level");
        assert!(algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_RECEIPT_V3), "and a receipt buys no position at all");
    }
}
