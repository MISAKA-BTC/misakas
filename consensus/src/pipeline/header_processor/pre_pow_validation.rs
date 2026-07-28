use super::*;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::block_window_cache::BlockWindowHeap;
use crate::processes::window::WindowManager;
use kaspa_consensus_core::header::Header;

impl HeaderProcessor {
    pub(super) fn pre_pow_validation(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        self.check_pruning_violation(ctx)?;
        self.check_difficulty_and_daa_score(ctx, header)?;
        Ok(())
    }

    fn check_pruning_violation(&self, ctx: &HeaderProcessingContext) -> BlockProcessResult<()> {
        let known_parents = ctx.known_direct_parents.as_slice();

        // We check that the new block is in the future of the pruning point by verifying that at least
        // one of its parents is in the pruning point future (or the pruning point itself). Otherwise,
        // the Prunality proof implies that the block can be discarded.
        if !self.reachability_service.is_dag_ancestor_of_any(ctx.pruning_point, &mut known_parents.iter().copied()) {
            return Err(RuleError::PruningViolation(ctx.pruning_point));
        }
        Ok(())
    }

    fn check_difficulty_and_daa_score(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let ghostdag_data = ctx.ghostdag_data();
        let daa_window = self.window_manager.block_daa_window(ghostdag_data)?;

        if daa_window.daa_score != header.daa_score {
            return Err(RuleError::UnexpectedHeaderDaaScore(daa_window.daa_score, header.daa_score));
        }

        // kaspa-pq ADR-0039 §16.3 / C6 clause 7 — LANE-AWARE difficulty. On a PALW-active net EVERY block
        // is v3 and each LANE retargets on its OWN blocks (the hash floor on algo-3 blocks, the replica
        // lane on algo-4 blocks), so a mixed-lane header's `bits` must match its lane's difficulty, not
        // the single-lane average over both lanes. The else-branch is BYTE-FOR-BYTE the pre-PALW path.
        // Mainnet, testnet-10, simnet and devnet use `palw_activation_daa_score == u64::MAX` and take
        // the legacy branch. The three PALW presets use 0 and take the lane-aware branch.
        let expected_bits = if header.daa_score >= self.palw_activation_daa_score {
            self.calculate_palw_lane_difficulty_bits(&daa_window.window, header.pow_algo_id)
        } else {
            self.window_manager.calculate_difficulty_bits(ghostdag_data, &daa_window)
        };
        ctx.mergeset_non_daa = Some(daa_window.mergeset_non_daa);

        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.hash, header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(daa_window.window);
        Ok(())
    }

    /// ADR-0039 §16.3 / C6 clause 7 — the expected `bits` for a v3 header's LANE, from the same DAA
    /// window filtered to same-lane blocks (each block's `pow_algo_id` read from its header — it is not
    /// in `CompactHeaderData`). Delegates the trim + retarget to the pure, live-engine-equivalent
    /// [`crate::processes::difficulty::lane_expected_bits`]. Below the lane's `min_samples` it HOLDs the
    /// lane's `genesis_bits` — a PURE header-window value (NOT the virtual, pruned lane-bits store, which
    /// would reintroduce the C6 order/prune hazard). Only reached inside the `palw_active` gate, so it
    /// never runs on a shipped preset.
    ///
    /// The body lives in [`crate::processes::difficulty::lane_bits_from_window`] so the algo-4 mining
    /// template derives `bits` through the SAME code this check runs — construction == validation for a
    /// field the miner does not get to choose.
    fn calculate_palw_lane_difficulty_bits(&self, window: &BlockWindowHeap, header_algo_id: u8) -> u32 {
        crate::processes::difficulty::lane_bits_from_window(
            self.headers_store.as_ref(),
            window,
            header_algo_id,
            &self.palw_lane_difficulty,
        )
    }
}
