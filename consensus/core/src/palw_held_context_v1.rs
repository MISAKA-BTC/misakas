//! **ADR-0103 — the context is held off the chain; the chain carries a root, an opening and a
//! logarithm.** The regime's pure half: what a seat's sampling unit is (Decision 2), which route a
//! seat takes to an interval's start (Decision 2), how wide an interval is (Decision 2, derived),
//! and what a seat holding a shard fetches to resume one (Decision 7). Consensus-inert: nothing
//! here is a consensus object or a fold rule; a seat's draw and its verification are the seat's
//! own duty (ADR-0077 Decision 8), and both the executor that opens an interval and the seat that
//! asks for it read these functions, so the two cannot describe different intervals.
//!
//! **A class is under the regime when it registered a held map** (`palw_state_chunk_map::
//! palw_profile_is_held_v4`): the marker is inside the class id, and the admission gate refuses such
//! a class unless `Params::palw_held_context` is armed.

use crate::palw_context_ladder::{PALW_COURT_COST_A16, PALW_COURT_COST_QWEN36, PalwCourtRowCostV1};
use crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3};

/// **The seat's sampling unit under the held regime: an interval of POSITIONS over the whole job,
/// prefill included** (ADR-0103 Decision 2).
///
/// The shipped unit partitions the DECODE calls, so interval 0 is the whole prefill — at 2M
/// positions a seat that draws it replays two million positions, and one that does not has checked
/// nothing of the prompt. Here the job's steps (the enumeration's own: step `s` is kv length `s`,
/// prefill positions first, then one step a decode call) are cut into runs of `width` positions;
/// interval `j` is steps `j·width + 1 ..= min((j+1)·width, steps)`, and every step is in exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHeldSeatIntervalsV1 {
    pub prefill: u32,
    /// `exact_decode_tokens − 1`.
    pub decode_calls: u32,
    /// `prefill + decode_calls` — the steps the enumeration walks.
    pub steps: u64,
    /// Positions an interval covers (`P`).
    pub width: u32,
    pub count: u32,
}

impl PalwHeldSeatIntervalsV1 {
    /// From the chain's two numbers and the class's derived width — the form a seat must use: an
    /// executor that could move the count could predict which intervals a draw lands on.
    pub fn from_chain_facts_v1(prompt_tokens: u32, decode_tokens_executed: u32, width: u32) -> Option<Self> {
        if width == 0 {
            return None;
        }
        let decode_calls = decode_tokens_executed.saturating_sub(1);
        let steps = u64::from(prompt_tokens) + u64::from(decode_calls);
        let count = u32::try_from(steps.div_ceil(u64::from(width)).max(1)).ok()?;
        Some(Self { prefill: prompt_tokens, decode_calls, steps, width, count })
    }

    /// `(first_step, last_step)` inclusive, kv lengths — `None` past the last interval.
    pub fn steps_for(&self, index: u32) -> Option<(u64, u64)> {
        if index >= self.count || self.steps == 0 {
            return None;
        }
        let first = u64::from(index) * u64::from(self.width) + 1;
        let last = (u64::from(index) + 1).saturating_mul(u64::from(self.width)).min(self.steps);
        (first <= last).then_some((first, last))
    }

    /// How many cache rows the state an interval RESUMES from holds: the positions before its
    /// first step. Interval 0 resumes from the prompt, which is zero rows.
    pub fn resume_positions(&self, index: u32) -> Option<u64> {
        self.steps_for(index).map(|(first, _)| first - 1)
    }

    /// The step `(call, position)` the enumeration names: prefill positions are call 0, a decode
    /// call is its own call at position 0.
    pub fn call_and_position(&self, step: u64) -> Option<(u32, u32)> {
        if step == 0 || step > self.steps {
            return None;
        }
        if step <= u64::from(self.prefill) {
            Some((0, u32::try_from(step - 1).ok()?))
        } else {
            Some((u32::try_from(step - u64::from(self.prefill)).ok()?, 0))
        }
    }
}

/// **Which route a seat takes to the start of an interval** (ADR-0103 Decision 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwHeldSeatRouteV1 {
    /// ADR-0082 Decision 9: recompute the cache from the prompt ids the seat holds. Chosen where
    /// the class's whole context fits the window at the family's replay rate.
    Recompute,
    /// Fetch the checkpoint chunks of the seat's own layers at the interval's start, verify each
    /// against the checkpoint's state root, and replay from them. Chosen where no window holds a
    /// recompute of the whole context.
    Resume,
}

/// The fraction of `window_receipt` a seat's work may take — the drill's stated margin, so a seat
/// on the slowest certified host still files inside the window with half of it to spare.
pub const PALW_HELD_SEAT_WINDOW_MARGIN_DIVISOR: u64 = 2;

/// **The family's measured replay row** — the one SA-4 derives every turn deadline from
/// (`palw_context_ladder::PALW_COURT_ROW_COSTS`), chosen by the graph's layer composition: a class
/// with recurrence layers replays at the hybrid tier's rate, every other integer class at the dense
/// tier's (which the floor's own row is bounded by).
pub fn palw_held_replay_row_v1(profile: &PalwShapeProfileV3) -> PalwCourtRowCostV1 {
    if (0..profile.layer_count).any(|l| profile.layer_kind(l) == PalwLayerKindV1::GatedDeltaNet) {
        PALW_COURT_COST_QWEN36
    } else {
        PALW_COURT_COST_A16
    }
}

/// A seat's budget in milliseconds: `window_receipt` at the frozen 120-second cadence, over the
/// margin.
pub fn palw_held_seat_budget_ms_v1(window_receipt_daa: u64) -> u64 {
    window_receipt_daa.saturating_mul(PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS) / PALW_HELD_SEAT_WINDOW_MARGIN_DIVISOR
}

/// **The route, derived from the class and the window** — `Recompute` exactly when replaying the
/// whole context at the family's rate fits the seat's budget.
pub fn palw_held_seat_route_v1(n_ctx: u32, replay_ms_per_position: u64, window_receipt_daa: u64) -> PalwHeldSeatRouteV1 {
    if u64::from(n_ctx).saturating_mul(replay_ms_per_position) <= palw_held_seat_budget_ms_v1(window_receipt_daa) {
        PalwHeldSeatRouteV1::Recompute
    } else {
        PalwHeldSeatRouteV1::Resume
    }
}

/// **The interval width `P`, derived** (ADR-0103 Decision 2): the largest power of two for which
/// the seat's fetch plus a replay of `P` positions fits its budget, and for which one interval's
/// opening (the fold's digests, one per 4,096 leaves) fits the transport cap the caller states —
/// never wider than `n_ctx`, never below 1. Wider is better coverage for the same draw
/// (ADR-0098: a one-token lie is caught with probability ≈ `s·k·P / steps`), so the widest the seat
/// can afford is the rule.
pub fn palw_held_seat_interval_positions_v1(
    n_ctx: u32,
    replay_ms_per_position: u64,
    fetch_ms: u64,
    window_receipt_daa: u64,
    leaves_per_position: u64,
    opening_cap_bytes: u64,
) -> u32 {
    let budget = palw_held_seat_budget_ms_v1(window_receipt_daa).saturating_sub(fetch_ms);
    let by_clock = if replay_ms_per_position == 0 { u64::MAX } else { budget / replay_ms_per_position };
    // One 64-byte digest per 4,096 leaves (ADR-0086 Decision 1's retention level).
    let by_wire = if leaves_per_position == 0 { u64::MAX } else { opening_cap_bytes.saturating_mul(4096) / 64 / leaves_per_position };
    let most = by_clock.min(by_wire).min(u64::from(n_ctx)).max(1);
    let p = 1u64 << (63 - most.leading_zeros());
    p as u32
}

/// **What a seat holding layers `layers` fetches to resume at `positions`** (ADR-0103 Decision 7):
/// the K and V rows of its attention layers for every position before the interval, and the whole
/// recurrent state of its recurrence layers. Linear in the context and held off the chain — the
/// one linear term the regime keeps, bounded here and in the plan.
pub fn palw_held_seat_fetch_bytes_v1(profile: &PalwShapeProfileV3, positions: u64, layers: std::ops::Range<u16>) -> u64 {
    let kv_row = u64::from(profile.attn_kv_heads).saturating_mul(u64::from(profile.attn_head_dim)).saturating_mul(4);
    let state = u64::from(profile.gdn_heads)
        .saturating_mul(u64::from(profile.gdn_head_k_dim))
        .saturating_mul(u64::from(profile.gdn_head_v_dim))
        .saturating_mul(4);
    let mut total = 0u64;
    for layer in layers.start..layers.end.min(profile.layer_count) {
        total = total.saturating_add(match profile.layer_kind(layer) {
            PalwLayerKindV1::Attention => positions.saturating_mul(kv_row).saturating_mul(2),
            PalwLayerKindV1::GatedDeltaNet => state,
        });
    }
    total
}

/// The fetch, as milliseconds at `bytes_per_second` — what [`palw_held_seat_interval_positions_v1`]
/// subtracts from the budget.
pub fn palw_held_fetch_ms_v1(bytes: u64, bytes_per_second: u64) -> u64 {
    if bytes_per_second == 0 {
        return u64::MAX;
    }
    bytes.saturating_mul(1000).div_ceil(bytes_per_second)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every step of a job is in exactly one interval, prefill included** (ADR-0103 Invariant 6's
    /// first half): interval 0 is `P` positions and never the whole prefill.
    #[test]
    fn every_step_is_in_exactly_one_interval_and_interval_zero_is_p_positions() {
        for (prefill, decode, width) in [(1u32, 1u32, 1u32), (7, 3, 4), (512, 2, 16), (2_000_000, 1025, 1024), (3, 9, 64)] {
            let g = PalwHeldSeatIntervalsV1::from_chain_facts_v1(prefill, decode, width).expect("a geometry");
            assert_eq!(g.count as u64, g.steps.div_ceil(width as u64).max(1));
            let mut next = 1u64;
            for j in 0..g.count {
                let (first, last) = g.steps_for(j).expect("in range");
                assert_eq!(first, next, "contiguous");
                assert!(last - first < width as u64, "at most P");
                next = last + 1;
                assert_eq!(g.resume_positions(j), Some(first - 1));
            }
            assert_eq!(next, g.steps + 1, "the last interval ends at the last step");
            let (first, last) = g.steps_for(0).expect("interval 0");
            assert_eq!(first, 1);
            assert_eq!(last, (width as u64).min(g.steps), "interval 0 is P positions, not the prefill");
            assert!(g.steps_for(g.count).is_none());
        }
        let g = PalwHeldSeatIntervalsV1::from_chain_facts_v1(3, 4, 2).expect("a geometry");
        assert_eq!(g.call_and_position(1), Some((0, 0)));
        assert_eq!(g.call_and_position(3), Some((0, 2)), "the last prefill position");
        assert_eq!(g.call_and_position(4), Some((1, 0)), "the first decode call");
        assert_eq!(g.call_and_position(6), Some((3, 0)));
        assert_eq!(g.call_and_position(7), None);
    }

    /// **The route and the width are derived, and the window binds them.** At the dense tier's
    /// 34 ms a position and a 600-DAA window, 512 positions recompute and two million resume; the
    /// width is a power of two, inside the transport, inside the clock, never past the context.
    #[test]
    fn the_route_and_the_width_are_derived_from_the_window() {
        let ms = PALW_COURT_COST_A16.replay_ms_per_position();
        assert_eq!(ms, 34);
        assert_eq!(palw_held_seat_route_v1(512, ms, 600), PalwHeldSeatRouteV1::Recompute);
        assert_eq!(palw_held_seat_route_v1(1 << 21, ms, 600), PalwHeldSeatRouteV1::Resume);
        let p = palw_held_seat_interval_positions_v1(1 << 21, ms, 0, 600, 103_008, 2 << 20);
        assert!(p.is_power_of_two());
        assert!(p as u64 * 103_008 / 4096 * 64 <= 2 << 20, "the opening fits the stated cap");
        assert!(p as u64 * ms <= palw_held_seat_budget_ms_v1(600), "the replay fits the budget");
        assert_eq!(palw_held_seat_interval_positions_v1(16, ms, 0, 600, 103_008, 2 << 20), 16, "never past the context");
        assert_eq!(palw_held_seat_interval_positions_v1(1 << 21, ms, u64::MAX, 600, 103_008, 2 << 20), 1, "never below one");
    }
}
