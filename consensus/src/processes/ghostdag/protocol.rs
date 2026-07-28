use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw::{COMPUTE_TO_HASH_CAP, PalwActiveNullifierSet};
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_REPLICA;
use kaspa_consensus_core::{
    BlockHashMap, BlockLevel, BlueWorkType, HashMapCustomHasher,
    blockhash::{self, BlockHashExtensions, BlockHashes},
};
use kaspa_hashes::Hash64;
use kaspa_utils::refs::Refs;

use crate::{
    model::{
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{GhostdagData, GhostdagStoreReader, HashKTypeMap, KType},
            headers::HeaderStoreReader,
            palw_nullifier::{DbPalwNullifierStore, PalwNullifierStoreReader},
            relations::RelationsStoreReader,
        },
    },
    processes::difficulty::{calc_work, level_work, normalize_palw_work},
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

    /// kaspa-pq ADR-0039 PALW activation fence (§15/§16). When the selected parent's DAA score is at or
    /// above this, the compute lane is live and `ghostdag` accumulates separated component work with
    /// nullifier dedup; below it the accumulation is the pre-PALW single-hash-work path,
    /// byte-identical. Mainnet, testnet-10, simnet and devnet use `u64::MAX`; the PALW presets use 0.
    /// Higher-level pruning-proof managers pass `u64::MAX` because PALW dedup applies only at level 0.
    palw_activation_daa_score: u64,
    /// Consensus-fixed compute-credit factor, independent from lane acceptance. Stage A uses zero so
    /// algo-4 blocks participate in coloring/DAA measurements without increasing fork-choice work.
    palw_compute_work_scale: u64,
    /// kaspa-pq ADR-0039 PALW (§15.2/§15.3) — the persistent per-block active-nullifier window store,
    /// threaded so the coloring dedup seed at a block covers **cross-ancestor** reuse (a block reusing a
    /// ticket nullifier buried in its selected parent's past, not only one in the current mergeset).
    /// `None` for the higher-level pruning-proof managers (they never run the PALW seed, `with_level`
    /// pins `palw_activation = u64::MAX`); `Some` only for the live level-0 manager. Read is gated on
    /// `palw_active`, so it stays untouched — and coloring byte-identical — on mainnet / testnet-10 /
    /// simnet / devnet. The three PALW presets use the persistent window from genesis.
    palw_nullifier_store: Option<Arc<DbPalwNullifierStore>>,
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> GhostdagManager<T, S, U, V> {
    pub fn new(
        genesis_hash: BlockHash,
        k: KType,
        ghostdag_store: Arc<T>,
        relations_store: S,
        headers_store: Arc<V>,
        reachability_service: U,
        palw_activation_daa_score: u64,
        palw_compute_work_scale: u64,
        palw_nullifier_store: Option<Arc<DbPalwNullifierStore>>,
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
            palw_activation_daa_score,
            palw_compute_work_scale,
            palw_nullifier_store,
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
    ) -> Self {
        Self {
            genesis_hash,
            k,
            ghostdag_store,
            relations_store,
            reachability_service,
            headers_store,
            level_work: level_work(level, max_block_level),
            // Pruning-proof / higher-level GD never runs the PALW component path (proofs reconstruct
            // work from header commitments); keep it inert here regardless of network.
            palw_activation_daa_score: u64::MAX,
            palw_compute_work_scale: 0,
            palw_nullifier_store: None,
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

    /// ADR-0039 §15.3 — the PALW ticket of a source block: `Some((ticket_nullifier, daa_score))` iff the
    /// block is on the algo-4 replica lane, else `None` (algo-3 hash blocks carry no ticket). Reads the
    /// full header and is called only on the PALW-active path.
    fn palw_ticket_of(&self, hash: BlockHash) -> Option<(Hash64, u64)> {
        let header = self.headers_store.get_header(hash).unwrap();
        if header.pow_algo_id == POW_ALGO_ID_PALW_REPLICA { Some((header.palw_ticket_nullifier, header.daa_score)) } else { None }
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

        // ADR-0039 §15/§16 activation gate over the complete direct-parent set. At the hard-fork
        // boundary the effective-work-selected parent can still be pre-v3 while another direct parent
        // is already a weight-zero v3/algo-4 block. Keying only on the selected parent would then count
        // that replica source as hash work. Any active direct parent is sufficient to enable per-lane
        // coloring/accumulation. Mainnet, testnet-10, simnet and devnet retain the inactive path,
        // while the PALW presets activate this branch from genesis.
        let palw_active =
            parents.iter().any(|parent| self.headers_store.get_daa_score(*parent).unwrap() >= self.palw_activation_daa_score);

        // §15.3 nullifier dedup, INTEGRATED into coloring (never a post-pass — that would break the blue
        // anticone bookkeeping). A first-seen algo-4 ticket is kept blue and its nullifier recorded; a
        // re-use is colored RED exactly like a k-cluster reject, so `add_red` keeps the anticone sizes
        // consistent. Seeded from the selected parent's PERSISTENT window (all nullifiers active in SP's
        // past + SP's mergeset, §15.2) PLUS SP's own ticket — because `window(SP)` is built from SP's
        // `mergeset_blues` and so excludes SP itself. This is the cross-ancestor seed: a block reusing a
        // ticket buried in SP's past (not in the current mergeset) is now recolored red. Empty / no-op
        // while inert. The window read is FAIL-CLOSED (a missing window for an active, non-genesis SP is a
        // consensus-state invariant break, matching the beacon's fail-closed policy) but boundary-aware:
        // an SP predating activation (or the re-genesis block itself) legitimately has no window ⇒ empty.
        let mut active_nullifiers = PalwActiveNullifierSet::new();
        if palw_active {
            if let Some(store) = &self.palw_nullifier_store {
                let sp_active = selected_parent != self.genesis_hash
                    && self.headers_store.get_daa_score(selected_parent).unwrap() >= self.palw_activation_daa_score;
                if sp_active {
                    let window = store.get(selected_parent).unwrap_or_else(|err| {
                        panic!("missing PALW nullifier window for active selected parent {selected_parent}: {err}")
                    });
                    active_nullifiers.merge_from(&window);
                }
            }
            if let Some((nf, daa)) = self.palw_ticket_of(selected_parent) {
                active_nullifiers.insert(nf, daa);
            }
        }

        for blue_candidate in ordered_mergeset.iter().cloned() {
            let coloring = self.check_blue_candidate(&new_block_data, blue_candidate, k);

            if let ColoringOutput::Blue(blue_anticone_size, blues_anticone_sizes) = coloring {
                // PALW duplicate-ticket rule: an algo-4 candidate whose nullifier is already active is a
                // double-use ⇒ red (non-creditable), else keep blue and register the nullifier.
                if palw_active
                    && let Some((nf, daa)) = self.palw_ticket_of(blue_candidate)
                    && !active_nullifiers.insert(nf, daa)
                {
                    new_block_data.add_red(blue_candidate);
                    continue;
                }
                // No k-cluster violation found, we can now set the candidate block as blue
                new_block_data.add_blue(blue_candidate, blue_anticone_size, &blues_anticone_sizes);
            } else {
                new_block_data.add_red(blue_candidate);
            }
        }

        let blue_score = self.ghostdag_store.get_blue_score(selected_parent).unwrap() + new_block_data.mergeset_blues.len() as u64;

        // ADR-0039 §5.3/§15.4: accumulate the two lanes separately over the (deduped) blue mergeset.
        let (blue_hash_work, blue_compute_work_raw) = if !palw_active {
            // Pre-PALW / inert: every source is on the algo-3 hash floor, so the hash term is the legacy
            // `Σ calc_work(bits).max(level_work)` and the compute term stays whatever the parent carried
            // (0). The finalizer then yields `blue_work = E = H + min(0, cap·H) = H`, byte-identical to
            // the pre-PALW single-work result; the per-field parent reads keep the legacy semantics.
            let added_hash_work: BlueWorkType = new_block_data
                .mergeset_blues
                .iter()
                .cloned()
                .map(|hash| calc_work(self.headers_store.get_bits(hash).unwrap()).max(self.level_work))
                .sum();
            let bhw = self.ghostdag_store.get_blue_hash_work(selected_parent).unwrap() + added_hash_work;
            let bcw = self.ghostdag_store.get_blue_compute_work(selected_parent).unwrap();
            (bhw, bcw)
        } else {
            // PALW active (§15.4): split each blue source's work by its lane — algo-3 → ΔH via
            // `calc_work`, unique algo-4 → ΔC via `normalize_palw_work` (same 32-bit-compact work unit,
            // never `calc_work_512`). Duplicates were already colored red, so the blue set is unique.
            let mut added_hash = BlueWorkType::from(0u64);
            let mut added_compute = BlueWorkType::from(0u64);
            for &blue in new_block_data.mergeset_blues.iter() {
                let header = self.headers_store.get_header(blue).unwrap();
                if header.pow_algo_id == POW_ALGO_ID_PALW_REPLICA {
                    // Canonical Compute v1 §17.5 fix 2 — model-as-data ACTIVATION SEAM. Today the scale is
                    // the flat const `palw_compute_work_scale` (the FORMULA `normalize_palw_work` + the cap
                    // stay in protocol). At activation this becomes the per-set VALUE:
                    // `normalize_palw_work(header.bits, kaspa_consensus_core::palw::resolve_compute_work_scale(
                    //      active_set_records, source_set_id, header.daa_score, self.palw_compute_work_scale))`
                    // — the ramped `effective_compute_work_scale()` of the source's set, falling back to this
                    // const when no record governs it. Left as the flat scalar here (this whole else-branch is
                    // dead while inert; wiring the record source in is a re-genesis / Header-v4 step).
                    added_compute = added_compute + normalize_palw_work(header.bits, self.palw_compute_work_scale);
                } else {
                    added_hash = added_hash + calc_work(header.bits).max(self.level_work);
                }
            }
            let bhw = self.ghostdag_store.get_blue_hash_work(selected_parent).unwrap() + added_hash;
            let bcw = self.ghostdag_store.get_blue_compute_work(selected_parent).unwrap() + added_compute;
            (bhw, bcw)
        };

        new_block_data.finalize_score_and_component_work(blue_score, blue_hash_work, blue_compute_work_raw, COMPUTE_TO_HASH_CAP);

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
