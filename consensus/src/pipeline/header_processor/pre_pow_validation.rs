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
        // **ADR-0142 retires the slot rule past `palw_clock_cursor`.**
        //
        // The slot rule existed to bound the lane's width. Past the cursor the clock is bounded
        // instead, and by something the economic lane cannot move: the heartbeat exemption is
        // granted at most once per interval of wall clock, so the DAA cannot run fast however many
        // beats are minted. (**Only past `palw_clock_floor`, it turned out** — the 2026-09-24
        // heartbeat audit, H5: that sentence leaned on the carried cursor's whole-slot advance, which
        // ADR-0142 §6a deleted, and nothing constrained the timestamp of the block that STEPS the
        // clock. The step rule below is what makes it true; see ADR-0142 §9.) What is left for the
        // slot rule to do is only harm — it is measured against the selected parent, the economic
        // lane refreshes that parent, and a chain producing faster than the interval starved the
        // lane completely. The width that remains is bounded where it always was: a fixed hash
        // price, and at most four beats a mergeset.
        //
        // So past this fence a beat may be minted whenever its producer can pay for it, and earns
        // the chain a DAA only when the cursor says a slot is open.
        //
        // **Past `palw_clock_floor` that last sentence is a rule (the 2026-09-24 heartbeat audit,
        // H3).** "Whenever its producer can pay" turned out to be the lane's whole waste: every beat
        // minted between two slots was a valid block that could never be granted — 89% of
        // testnet-12's — each adding a blue score and a relay. So a heartbeat stamped before the
        // slot its own window's cursor opens is invalid. The cursor is the one the DAA score was
        // computed with (`DaaWindow::clock`), derived from this header's own parents, so every node
        // answers alike; and it is the slot this beat could be GRANTED, so an honest beat — which
        // the adapter stamps at `max(now, slot)` — never fails it, whatever its miner's clock says.
        // A miner whose clock runs behind stamps the slot itself (up to the future-drift tolerance
        // ahead of its own clock) rather than an earlier time, so skew costs it a wait, never a
        // refusal. No margin below the slot is granted, because a beat below it earns nothing.
        let clock = daa_window.clock;
        if clock.floor
            && header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
            && let Err(early) = clock.heartbeat_stamp_admits(header.timestamp)
        {
            return Err(RuleError::HeartbeatBeforeItsSlot(header.hash, header.timestamp, early.next_slot_ms));
        }
        // **H5: the block that STEPS the clock is stamped at or past the slot it consumed.** It
        // becomes the next slot's reference, and the drift tolerance (132 s) is longer than the
        // interval (120 s): a beat stamped `ref + 120 s` was granted the moment the reference existed,
        // and a step stamped "now" became the next reference — nothing floored the spacing between two
        // ticks. With this, every reference is at least one interval after one at the score before, so
        // a producer's drift buys at most one slot of lead, once, and never a rate. The builder stamps
        // a step at `max(now, median + 1, slot)`, so an honest one never meets this refusal.
        if let Err(early) = clock.step_stamp_admits(header.timestamp) {
            return Err(RuleError::ClockStepBeforeItsSlot(header.hash, header.timestamp, early.next_slot_ms));
        }
        let clock_cursor_governs = self.palw_clock_cursor.is_some_and(|fence| fence.is_active(header.daa_score));
        if !clock_cursor_governs
            && header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
            && self.palw_heartbeat_lane.is_some_and(|fence| fence.is_active(header.daa_score))
        {
            let parent = self
                .headers_store
                .get_header(ghostdag_data.selected_parent)
                .map_err(|_| RuleError::MissingParents(vec![ghostdag_data.selected_parent]))?;
            // ADR-0138 §3c: the interval follows the CLOCK, not the bond. Past `palw_anchor_clock`
            // a bonded attempt parent produces without advancing the DAA, so backing off an hour
            // for it leaves a PALW-only chain with no clock at all. Both arguments are pure
            // functions of stored headers and the preset's fences — no store read, no local fact.
            let anchor_clock_active = self.palw_anchor_clock.is_some_and(|fence| fence.is_active(header.daa_score));
            let parent_advances_daa = crate::processes::difficulty::palw_lane_advances_daa_v1(
                parent.pow_algo_id,
                parent.daa_score,
                self.palw_anchor_clock,
                self.palw_single_lottery,
                self.palw_receipt_rows_unpriced,
            );
            if let Err(early) = kaspa_consensus_core::palw_heartbeat_v1::check_heartbeat_slot_v2(
                parent.timestamp,
                parent.pow_algo_id,
                anchor_clock_active,
                parent_advances_daa,
                header.timestamp,
            ) {
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
