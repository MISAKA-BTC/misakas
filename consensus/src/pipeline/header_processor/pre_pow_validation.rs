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

        // ADR-0060 Decision 1: on a `ConsensusV2` network a heartbeat (algo-3) header's `bits`
        // are the LANE's own retarget, not the global one — the global bits on a V2 network sit
        // near the trivial genesis value (the class lottery is the real throttle there), and a
        // hash lane priced at them would cost ~nothing per block. The lane's difficulty and its
        // slot rule are pure functions of chain-order evidence walked from this POV.
        //
        // The global calculation below deliberately still sees heartbeat rows (their harder bits
        // pull the average ≤ its cadence share, ~3.3‰..33‰): a filtered average would need the
        // algo id in the compact-header store, and the pollution is bounded and self-correcting
        // — during a ramp there is no bonded miner to burden, and afterwards the window re-mixes.
        let expected_bits = if header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
            && matches!(self.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_))
        {
            // Chain-order evidence, NOT the (sampled) difficulty window: the sampled window can
            // miss the newest blocks entirely, and the slot rule is about exactly those.
            let rows = crate::processes::heartbeat_evidence::collect_heartbeat_evidence(
                self.ghostdag_store.as_ref(),
                self.headers_store.as_ref(),
                ghostdag_data.selected_parent,
                self.genesis.hash,
            );
            if let Err(early) = kaspa_consensus_core::palw_heartbeat_v1::check_heartbeat_slot(&rows, header.timestamp) {
                return Err(RuleError::HeartbeatTooEarly(
                    header.hash,
                    header.timestamp,
                    early.last_heartbeat_timestamp,
                    early.interval_ms,
                ));
            }
            kaspa_consensus_core::palw_heartbeat_v1::heartbeat_expected_bits(&rows, header.timestamp)
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
}
