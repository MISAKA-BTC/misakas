use super::{HeaderProcessingContext, HeaderProcessor};
use crate::errors::{BlockProcessResult, RuleError, TwoDimVecDisplay};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::headers::HeaderStoreReader;
use crate::processes::window::WindowManager;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::header::Header;
use std::collections::HashSet;

impl HeaderProcessor {
    pub fn post_pow_validation(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        self.check_blue_score(ctx, header)?;
        self.check_blue_work(ctx, header)?;
        self.check_median_timestamp(ctx, header)?;
        self.check_mergeset_size_limit(ctx)?;
        self.check_mergeset_heartbeat_width(ctx, header)?;
        self.check_bounded_merge_depth(ctx)?;
        self.check_indirect_parents(ctx, header)
    }

    pub fn check_median_timestamp(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let (past_median_time, window) = self.window_manager.calc_past_median_time(ctx.ghostdag_data())?;
        ctx.block_window_for_past_median_time = Some(window);

        if header.timestamp <= past_median_time {
            return Err(RuleError::TimeTooOld(header.timestamp, past_median_time));
        }

        Ok(())
    }

    pub fn check_mergeset_size_limit(&self, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let mergeset_size = ctx.ghostdag_data().mergeset_size() as u64;
        let mergeset_size_limit = self.mergeset_size_limit;
        if mergeset_size > mergeset_size_limit {
            return Err(RuleError::MergeSetTooBig(mergeset_size, mergeset_size_limit));
        }
        Ok(())
    }

    /// **F3a's bound — a mergeset may hold at most `PALW_HEARTBEAT_MAX_PER_MERGESET` heartbeat
    /// blocks** (ADR-0068 Phase 1, closing what ADR-0066 recorded open).
    ///
    /// The slot rule bounds the CHAIN (one heartbeat per interval behind its selected parent) and
    /// the fixed price bounds the header rate, but sibling heartbeats share one selected parent
    /// and one admissible timestamp, so nothing bounded how many the DAG accepts. The bound lives
    /// here, beside `check_mergeset_size_limit`, because it is the same kind of rule: a property
    /// of the accepting block's mergeset, derived deterministically from its parents — no walk,
    /// no window, no node-local fact. A sibling flood is absorbed at a bounded rate (the template
    /// builder chunks it exactly as it chunks `mergeset_size_limit`), and a flood older than the
    /// merge depth is simply never merged.
    ///
    /// Gated on the heartbeat lane's own fence at the ACCEPTING header's daa score: before the
    /// fence no heartbeat header is admitted at all, so the bound arms exactly when the lane
    /// does — a lane must never exist without its width bound.
    pub fn check_mergeset_heartbeat_width(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        if !self.palw_heartbeat_lane.is_some_and(|fence| fence.is_active(header.daa_score)) {
            return Ok(());
        }
        let bound = kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET;
        let ghostdag_data = ctx.ghostdag_data();
        let mut heartbeats = 0u64;
        for member in ghostdag_data.mergeset_blues.iter().chain(ghostdag_data.mergeset_reds.iter()) {
            let member_header = self.headers_store.get_header(*member).unwrap();
            if member_header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
                heartbeats += 1;
                if heartbeats > bound {
                    return Err(RuleError::MergeSetTooManyHeartbeats(heartbeats, bound));
                }
            }
        }
        Ok(())
    }

    fn check_blue_score(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let gd_blue_score = ctx.ghostdag_data().blue_score;
        if gd_blue_score != header.blue_score {
            return Err(RuleError::UnexpectedHeaderBlueScore(gd_blue_score, header.blue_score));
        }
        Ok(())
    }

    fn check_blue_work(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let gd_blue_work = ctx.ghostdag_data().blue_work;
        if gd_blue_work != header.blue_work {
            return Err(RuleError::UnexpectedHeaderBlueWork(gd_blue_work, header.blue_work));
        }
        Ok(())
    }

    pub fn check_indirect_parents(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let expected_block_parents = self.parents_manager.calc_block_parents(ctx.pruning_point, header.direct_parents());
        if header.parents_by_level.expanded_len() != expected_block_parents.expanded_len()
            || !expected_block_parents.expanded_iter().zip(header.parents_by_level.expanded_iter()).all(
                |(expected_level_parents, header_level_parents)| {
                    if header_level_parents.len() != expected_level_parents.len() {
                        return false;
                    }
                    // Optimistic path where both arrays are identical also in terms of order
                    if header_level_parents == expected_level_parents {
                        return true;
                    }
                    HashSet::<&BlockHash>::from_iter(header_level_parents) == HashSet::<&BlockHash>::from_iter(expected_level_parents)
                },
            )
        {
            return Err(RuleError::UnexpectedIndirectParents(
                TwoDimVecDisplay(expected_block_parents.into()),
                TwoDimVecDisplay((&header.parents_by_level).into()),
            ));
        };
        Ok(())
    }

    pub fn check_bounded_merge_depth(&self, ctx: &mut HeaderProcessingContext) -> BlockProcessResult<()> {
        let ghostdag_data = ctx.ghostdag_data();
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(ghostdag_data, ctx.pruning_point);
        let finality_point = self.depth_manager.calc_finality_point(ghostdag_data, ctx.pruning_point);
        let mut kosherizing_blues: Option<Vec<BlockHash>> = None;

        for red in ghostdag_data.mergeset_reds.iter().copied() {
            if self.reachability_service.is_dag_ancestor_of(merge_depth_root, red) {
                continue;
            }
            // Lazy load the kosherizing blocks since this case is extremely rare
            if kosherizing_blues.is_none() {
                kosherizing_blues = Some(self.depth_manager.kosherizing_blues(ghostdag_data, merge_depth_root).collect());
            }
            if !self.reachability_service.is_dag_ancestor_of_any(red, &mut kosherizing_blues.as_ref().unwrap().iter().copied()) {
                return Err(RuleError::ViolatingBoundedMergeDepth);
            }
        }

        ctx.merge_depth_root = Some(merge_depth_root);
        ctx.finality_point = Some(finality_point);
        Ok(())
    }
}
