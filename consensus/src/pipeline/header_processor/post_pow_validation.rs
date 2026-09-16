use super::{HeaderProcessingContext, HeaderProcessor};
use crate::errors::{BlockProcessResult, RuleError, TwoDimVecDisplay};
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use crate::processes::window::WindowManager;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::blockhash::BlockHashExtensions;
use kaspa_consensus_core::header::Header;
use std::collections::HashSet;

impl HeaderProcessor {
    pub fn post_pow_validation(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        self.check_blue_score(ctx, header)?;
        self.check_blue_work(ctx, header)?;
        self.check_median_timestamp(ctx, header)?;
        self.check_mergeset_size_limit(ctx)?;
        self.check_round_lane_mergeset(ctx, header)?;
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
        // ADR-0125: round blocks are counted by the round lane's own bound
        // (`check_round_lane_mergeset`), not by this one — which keeps bounding the chain's own
        // blocks exactly as it did before the lane existed.
        let round_members = self.round_lane_members(ctx.ghostdag_data())?.len() as u64;
        let mergeset_size = ctx.ghostdag_data().mergeset_size() as u64 - round_members;
        let mergeset_size_limit = self.mergeset_size_limit;
        if mergeset_size > mergeset_size_limit {
            return Err(RuleError::MergeSetTooBig(mergeset_size, mergeset_size_limit));
        }
        Ok(())
    }

    /// ADR-0125: the round blocks of a mergeset as `(round, permit index)`, in mergeset order. Round
    /// blocks are always red, so only the reds are read — and nothing at all on a network that has
    /// not configured the lane.
    fn round_lane_members(
        &self,
        ghostdag_data: &crate::model::stores::ghostdag::GhostdagData,
    ) -> BlockProcessResult<Vec<(u64, u16, kaspa_consensus_core::palw_state_v2::PalwBondKeyV2)>> {
        let mut members = Vec::new();
        if self.palw_execution_lane.is_none() {
            return Ok(members);
        }
        for red in ghostdag_data.mergeset_reds.iter().copied() {
            let red_header = self.headers_store.get_header(red).map_err(|_| RuleError::MissingParents(vec![red]))?;
            if red_header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1 {
                continue;
            }
            let envelope = kaspa_consensus_core::palw_execution_lane_v1::PalwExecEnvelopeV1::decode(&red_header.palw_commitment)
                .map_err(|e| RuleError::BadRoundLaneMergeset(e.to_string()))?;
            members.push((envelope.round, envelope.permit_index, envelope.bond));
        }
        Ok(members)
    }

    /// **ADR-0125: the round lane's header rule.**
    ///
    /// * **The anchor rule.** Every round parent's anchor (its selected parent) lies on this block's
    ///   selected chain. So a round block's past brings no chain block into any mergeset that its
    ///   anchor's past had not already brought — and the chain's GHOSTDAG, reachability, DAA score
    ///   and depths are those of the DAG without the lane.
    /// * **The mergeset rule** ([`kaspa_consensus_core::palw_execution_lane_v1::palw_execution_mergeset_rule_v1`]):
    ///   at most `max_per_mergeset` round blocks, at most the width in force at this block's DAA score
    ///   of one round, one per permit, and a round block merges only rounds older than its own. A
    ///   mergeset reaches back only to spans at or before this block's, and a lane only widens
    ///   (§7.2), so the width here bounds every round a member can belong to.
    ///
    /// A property of this block's parents and mergeset headers alone — no walk, no state — like the
    /// heartbeat width rule beside it.
    pub fn check_round_lane_mergeset(&self, ctx: &mut HeaderProcessingContext, header: &Header) -> BlockProcessResult<()> {
        let Some(lane) = self.palw_execution_lane else {
            return Ok(());
        };
        let round_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1;
        let selected_parent = ctx.ghostdag_data().selected_parent;
        for parent in header.direct_parents().iter().copied() {
            if parent.is_origin() {
                continue;
            }
            let parent_header = self.headers_store.get_header(parent).map_err(|_| RuleError::MissingParents(vec![parent]))?;
            if parent_header.pow_algo_id != round_id {
                continue;
            }
            let anchor = self.ghostdag_store.get_selected_parent(parent).map_err(|_| RuleError::MissingParents(vec![parent]))?;
            if !self.reachability_service.is_chain_ancestor_of(anchor, selected_parent) {
                return Err(RuleError::BadRoundLaneParents(format!(
                    "round parent {parent} is anchored at {anchor}, which is not on this block's selected chain (selected parent {selected_parent})"
                )));
            }
        }
        let block_round = if header.pow_algo_id == round_id {
            let envelope = kaspa_consensus_core::palw_execution_lane_v1::PalwExecEnvelopeV1::decode(&header.palw_commitment)
                .map_err(|e| RuleError::BadRoundLaneMergeset(e.to_string()))?;
            Some(envelope.round)
        } else {
            None
        };
        let members = self.round_lane_members(ctx.ghostdag_data())?;
        kaspa_consensus_core::palw_execution_lane_v1::palw_execution_mergeset_rule_v1(
            block_round,
            &members,
            lane.width_at_daa(header.daa_score),
            lane.max_per_mergeset,
        )
        .map_err(|e| RuleError::BadRoundLaneMergeset(e.to_string()))
    }

    /// **F3a's bound, as the drill amended it — a mergeset may hold at most
    /// `PALW_HEARTBEAT_MAX_PER_MERGESET` heartbeat blocks, UNLESS they form one chain**
    /// (ADR-0068 Phase 1; the chain exemption closes drill finding F5).
    ///
    /// The slot rule bounds the chain's rate and the fixed price bounds the header rate, but
    /// sibling heartbeats share one selected parent and one admissible timestamp, so nothing
    /// bounded how many the DAG accepts. The bound lives here, beside
    /// `check_mergeset_size_limit`, because it is the same kind of rule: a property of the
    /// accepting block's mergeset — no walk, no window, no node-local fact.
    ///
    /// **Why the chain exemption exists (F5).** The live drill stranded five honest outage
    /// heartbeats PERMANENTLY: a heavier bonded fork put them in the anticone, and merging the
    /// tip would drag all five ancestors into one mergeset — over the flat bound, so 400+
    /// successive templates correctly refused, and nothing ever could absorb them (the
    /// intermediates are not tips, so no chunking path exists). But a heartbeat CHAIN is the
    /// lane doing exactly its job through a long outage — one block per slot, totally ordered —
    /// and its length is already priced by the slot ladder. What F3a is actually about is
    /// WIDTH: siblings, which the slot rule cannot see. So the rule is width-shaped now:
    ///
    /// * count ≤ bound → fine (the common case, no reachability asked);
    /// * count > bound → every pair of heartbeat members must be ancestor-related, i.e. the
    ///   members form ONE chain under reachability. Sorted by blue score, that is one
    ///   ancestor query per adjacent pair. A tree or a sibling layer riding a chain fails the
    ///   pairwise check — a root with many children is one head but arbitrary WIDTH, which is
    ///   why counting "chain heads" would have re-opened F3a and total order is required
    ///   instead.
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
        let mut heartbeats = Vec::new();
        for member in ghostdag_data.mergeset_blues.iter().chain(ghostdag_data.mergeset_reds.iter()) {
            let member_header = self.headers_store.get_header(*member).unwrap();
            if member_header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
                heartbeats.push((member_header.blue_score, *member));
            }
        }
        if heartbeats.len() as u64 <= bound {
            return Ok(());
        }
        // Over the flat bound: admissible only as one chain. Ancestor-relatedness is a total
        // order on a chain and blue score respects it, so sorting by blue score and asking one
        // reachability query per adjacent pair decides the whole set. (Ties in blue score are
        // possible only for blocks that are NOT ancestor-related, so a tie already fails.)
        heartbeats.sort_unstable_by_key(|(blue_score, _)| *blue_score);
        for pair in heartbeats.windows(2) {
            let ((_, older), (_, newer)) = (pair[0], pair[1]);
            if !self.reachability_service.is_dag_ancestor_of(older, newer) {
                return Err(RuleError::MergeSetTooManyHeartbeats(heartbeats.len() as u64, bound));
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
