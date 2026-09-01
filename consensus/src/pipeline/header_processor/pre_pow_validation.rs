use super::*;
use crate::errors::{BlockProcessResult, RuleError};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::headers::HeaderStoreReader;
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

        // **ADR-0066 Decision 1: a heartbeat header's `bits` are the GLOBAL expected bits, like
        // every other lane's.** There is no lane retarget any more, and that is the fix rather
        // than a simplification of it.
        //
        // The withdrawn design gave heartbeat headers the lane's own 2²⁴-hard `bits`, and those
        // rows sat in the global difficulty window. A V2 network's ambient target is
        // `MAX_DIFFICULTY_TARGET` because the class lottery is its throttle, so a window that
        // filled with heartbeat rows demanded work 33,554,432 and no bonded block could re-enter
        // it — the average never re-mixed and the chain was heartbeat-only for good. The lane's
        // price now lives in `StateLayer0::new` as a network constant, where nothing averages it.
        //
        // What remains here is the slot rule, and it reads the SELECTED PARENT alone. The old one
        // walked chain-order evidence and terminated on `Err(get_header)` — a node-local fact, so
        // an archival node and a pruned node computed different verdicts for the same header and
        // rejected each other along the `--archival` flag.
        // ADR-0071 Decision 1 froze this for a `ConsensusV2` network and the ADR now records why
        // that was wrong: the window's answer IS the block interval, and nothing else sets it.
        let expected_bits = self.window_manager.calculate_difficulty_bits(ghostdag_data, &daa_window);
        if header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
            && self.palw_heartbeat_lane.is_some_and(|fence| fence.is_active(header.daa_score))
        {
            let parent = self
                .headers_store
                .get_header(ghostdag_data.selected_parent)
                .map_err(|_| RuleError::MissingParents(vec![ghostdag_data.selected_parent]))?;
            if let Err(early) =
                kaspa_consensus_core::palw_heartbeat_v1::check_heartbeat_slot(parent.timestamp, parent.pow_algo_id, header.timestamp)
            {
                return Err(RuleError::HeartbeatTooEarly(
                    header.hash,
                    header.timestamp,
                    early.last_heartbeat_timestamp,
                    early.interval_ms,
                ));
            }
        }
        ctx.mergeset_non_daa = Some(daa_window.mergeset_non_daa);

        if header.bits != expected_bits {
            return Err(RuleError::UnexpectedDifficulty(header.hash, header.bits, expected_bits));
        }

        ctx.block_window_for_difficulty = Some(daa_window.window);
        Ok(())
    }
}
