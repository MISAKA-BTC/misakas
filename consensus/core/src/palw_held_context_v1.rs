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

/// **How many rows of history cost as much as one position's own work** (ADR-0103 §10.6), read
/// off the geometry the class registered.
///
/// A position replayed against `p` rows of history pays for its projections and its MLP, and then
/// for the attention over those `p` rows: `2 × heads × head_dim` multiply-accumulates a row in each
/// attention layer (the scores, and the weighted values). The knee is the first over the second.
/// The seat's cost at history `p` is then `(1 + p / knee)` positions' worth, so replaying a context
/// of `n` positions from nothing costs `n + n(n − 1) / 2knee` — a line only while `n` is small
/// against the knee. ADR-0110 §9.3 measured the curve this describes: from 4,096 to 32,768
/// positions the seat's recompute grew 41 times.
///
/// Counted per layer: an attention layer's `q`, `k`, `v` and `o` projections and its MLP in the
/// numerator, and its history row in the denominator. A recurrence layer's state is constant in
/// the history, so it adds its MLP and nothing to the denominator. The unembedding is left out,
/// because a seat's recompute drops the logits. Leaving work out of the numerator can only make the
/// knee SMALLER and the history dearer, which is the side a bound must err on. With no attention
/// layer the history costs nothing, and the knee is `u64::MAX`.
pub fn palw_held_attention_knee_v1(profile: &PalwShapeProfileV3) -> u64 {
    let hidden = u128::from(profile.hidden_dim);
    let q = u128::from(profile.attn_heads).saturating_mul(u128::from(profile.attn_head_dim));
    let kv = u128::from(profile.attn_kv_heads).saturating_mul(u128::from(profile.attn_head_dim));
    let mlp = hidden.saturating_mul(u128::from(profile.ffn_dim)).saturating_mul(3);
    let (mut own, mut history) = (0u128, 0u128);
    for layer in 0..profile.layer_count {
        match profile.layer_kind(layer) {
            PalwLayerKindV1::Attention => {
                // q and o are `hidden × q` each; k and v are `hidden × kv` each.
                let projections = hidden.saturating_mul(q.saturating_add(kv)).saturating_mul(2);
                own = own.saturating_add(projections).saturating_add(mlp);
                history = history.saturating_add(q.saturating_mul(2));
            }
            PalwLayerKindV1::GatedDeltaNet => own = own.saturating_add(mlp),
        }
    }
    if history == 0 {
        return u64::MAX;
    }
    u64::try_from((own / history).max(1)).unwrap_or(u64::MAX)
}

/// **The seat's replay, priced by where in the context it runs** (ADR-0103 Decision 2, as amended
/// in §10.6).
///
/// The family's measured rate prices a position with an empty history, which is how it was
/// measured: decode at an interactive context. Every `knee_positions` rows of history add one more
/// position's worth. Replaying a whole context therefore grows with its square once the context is
/// past the knee. The route, the width's clock and the shard plan all read this, so none of them
/// can price the same replay as a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHeldReplayCostV1 {
    /// The family's rate for one position (`palw_held_replay_row_v1`), taken as its cost with
    /// nothing before it.
    pub ms_per_position: u64,
    /// [`palw_held_attention_knee_v1`] of the class.
    pub knee_positions: u64,
}

impl PalwHeldReplayCostV1 {
    /// The class's cost: its family's measured row, and its own knee.
    pub fn for_profile_v1(profile: &PalwShapeProfileV3) -> Self {
        Self {
            ms_per_position: palw_held_replay_row_v1(profile).replay_ms_per_position(),
            knee_positions: palw_held_attention_knee_v1(profile),
        }
    }

    /// Milliseconds to replay the `count` positions that start at history length `first` — each
    /// against every row before it — rounded up, and saturating at `u64::MAX` (which fits no
    /// budget).
    ///
    /// `Σ_{p = first}^{first + count − 1} (1 + p / knee) = count + (count·first + count(count − 1)/2) / knee`.
    pub fn replay_ms_v1(&self, first: u64, count: u64) -> u64 {
        if self.knee_positions == u64::MAX {
            // No attention layer: the history costs nothing, and the replay is the line exactly.
            return count.saturating_mul(self.ms_per_position);
        }
        let knee = u128::from(self.knee_positions.max(1));
        let count = u128::from(count);
        // The rows of history the `count` positions attend to, summed: `count(count − 1)` is a
        // product of consecutive integers, so the halving is exact.
        let history = count.saturating_mul(u128::from(first)).saturating_add(count.saturating_mul(count.saturating_sub(1)) / 2);
        let positions_in_knees = count.saturating_mul(knee).saturating_add(history);
        u64::try_from(positions_in_knees.saturating_mul(u128::from(self.ms_per_position)).div_ceil(knee)).unwrap_or(u64::MAX)
    }
}

/// **The route, derived from the class and the window** — `Recompute` exactly when replaying the
/// whole context, its history priced (§10.6), fits the seat's budget.
pub fn palw_held_seat_route_v1(n_ctx: u32, cost: PalwHeldReplayCostV1, window_receipt_daa: u64) -> PalwHeldSeatRouteV1 {
    if cost.replay_ms_v1(0, u64::from(n_ctx)) <= palw_held_seat_budget_ms_v1(window_receipt_daa) {
        PalwHeldSeatRouteV1::Recompute
    } else {
        PalwHeldSeatRouteV1::Resume
    }
}

/// **The interval lane's transport cap**, in bytes — `protocol/flows`'s
/// `PALW_INTERVAL_OPENING_MAX_BYTES`, mirrored here because consensus-core cannot read the flows
/// crate and the width derivation below must be stated against the cap a seat will actually meet
/// (the flows crate pins the two equal). An interval's opening is the fold's digests over its
/// leaves (ADR-0086 Decision 1), so this bounds `P` from the wire side; a RESUME opening is not an
/// interval opening and is not bounded by it (ADR-0103 §4: the class declares its own, off the plan).
pub const PALW_HELD_SEAT_INTERVAL_OPENING_CAP_BYTES_V1: u64 = 4 << 20;

/// **The interval width `P`, derived** (ADR-0103 Decision 2): the largest power of two for which
/// the seat's fetch plus a replay of the LAST interval fits its budget, and for which one
/// interval's opening (the fold's digests, one per 4,096 leaves) fits the transport cap the caller
/// states — never wider than `n_ctx`, never below 1. The last interval is the dearest: its `P`
/// positions each attend to the `n_ctx − P` rows before them, and its price is
/// [`PalwHeldReplayCostV1::replay_ms_v1`]'s (§10.6). Wider is better coverage for the same draw
/// (ADR-0098: a one-token lie is caught with probability ≈ `s·k·P / steps`), so the widest the seat
/// can afford is the rule.
pub fn palw_held_seat_interval_positions_v1(
    n_ctx: u32,
    cost: PalwHeldReplayCostV1,
    fetch_ms: u64,
    window_receipt_daa: u64,
    leaves_per_position: u64,
    opening_cap_bytes: u64,
) -> u32 {
    let budget = palw_held_seat_budget_ms_v1(window_receipt_daa).saturating_sub(fetch_ms);
    let n = u64::from(n_ctx);
    // One 64-byte digest per 4,096 leaves (ADR-0086 Decision 1's retention level).
    let by_wire = if leaves_per_position == 0 { u64::MAX } else { opening_cap_bytes.saturating_mul(4096) / 64 / leaves_per_position };
    let most = by_wire.min(n).max(1);
    // The widest the wire and the context allow, halved until the last interval replays in time.
    // The replay is increasing in both its start and its width, so the first fit is the widest.
    let mut p = 1u64 << (63 - most.leading_zeros());
    while p > 1 && cost.replay_ms_v1(n.saturating_sub(p), p) > budget {
        p >>= 1;
    }
    p as u32
}

/// **The class's interval width `P`, the number the executor and every seat must agree on**
/// (ADR-0103 Decision 2) — a function of the CLASS and the transport, never of a host.
///
/// The widest power of two whose opening (one 64-byte digest a 4,096 leaves, ADR-0086 Decision 1)
/// fits the interval lane's cap, and which cuts the class's whole context into at least the draw's
/// `k` intervals (`PALW_FP_SEAT_INTERVAL_SAMPLES_V1`): a context that fit fewer than `k` intervals
/// would be drawn whole by every seat, and the point of the unit is that a seat checks a sample of
/// the positions, the prompt's included. Decision 2 derives `P` "at certification" from
/// the slowest certified seat's fetch and replay; a certification-time number that the executor
/// opens by and the seat draws over would have to be a consensus object before either could read
/// it, so the chain-facing width is this class function and the CLOCK is the certification's check:
/// the drill certifies the class only where its slowest seat resumes a `P`-position interval inside
/// `window_receipt` ([`palw_held_seat_interval_positions_v1`] is that check's arithmetic), and a seat
/// that cannot answers `Incapable`.
pub fn palw_held_interval_positions_v1(profile: &PalwShapeProfileV3) -> u32 {
    let n_ctx = u64::from(profile.n_ctx.max(1));
    let leaves = crate::palw_step::worst_case_step_leaf_count_capped_v1(profile, u64::MAX).unwrap_or(u64::MAX);
    let leaves_per_position = leaves.div_ceil(n_ctx).max(1);
    let by_wire = PALW_HELD_SEAT_INTERVAL_OPENING_CAP_BYTES_V1.saturating_mul(4096) / 64 / leaves_per_position;
    let by_draw = n_ctx / u64::from(crate::palw_fp_interval_v1::PALW_FP_SEAT_INTERVAL_SAMPLES_V1.max(1));
    let most = by_wire.min(by_draw).max(1);
    (1u64 << (63 - most.leading_zeros())) as u32
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
        let cost = PalwHeldReplayCostV1 { ms_per_position: ms, knee_positions: 15_232 };
        assert_eq!(palw_held_seat_route_v1(512, cost, 600), PalwHeldSeatRouteV1::Recompute);
        assert_eq!(palw_held_seat_route_v1(1 << 21, cost, 600), PalwHeldSeatRouteV1::Resume);
        let p = palw_held_seat_interval_positions_v1(1 << 21, cost, 0, 600, 103_008, 2 << 20);
        assert!(p.is_power_of_two());
        assert!(p as u64 * 103_008 / 4096 * 64 <= 2 << 20, "the opening fits the stated cap");
        assert!(cost.replay_ms_v1((1 << 21) - p as u64, p as u64) <= palw_held_seat_budget_ms_v1(600), "the last interval fits");
        assert_eq!(palw_held_seat_interval_positions_v1(16, cost, 0, 600, 103_008, 2 << 20), 16, "never past the context");
        assert_eq!(palw_held_seat_interval_positions_v1(1 << 21, cost, u64::MAX, 600, 103_008, 2 << 20), 1, "never below one");
    }

    /// **The history is priced** (§10.6). With no attention the replay is the old line to the
    /// millisecond; with a knee of `k` rows, `n` positions from nothing cost `n + n(n − 1)/2k`
    /// positions' worth, and the same positions later in the context cost more.
    #[test]
    fn the_replay_prices_the_history_and_is_a_line_only_without_attention() {
        let line = PalwHeldReplayCostV1 { ms_per_position: 34, knee_positions: u64::MAX };
        for n in [1u64, 512, 1 << 21] {
            assert_eq!(line.replay_ms_v1(0, n), n * 34, "no history term");
            assert_eq!(line.replay_ms_v1(1 << 20, n), n * 34, "and none later either");
        }
        let cost = PalwHeldReplayCostV1 { ms_per_position: 34, knee_positions: 28 };
        // 512 + 512·511/56 = 512 + 4,672 positions, at 34 ms.
        assert_eq!(cost.replay_ms_v1(0, 512), (512 + 4_672) * 34);
        // One position at history `p` is `1 + p/28` positions' worth, rounded up once.
        assert_eq!(cost.replay_ms_v1(28, 1), 2 * 34);
        assert_eq!(cost.replay_ms_v1(29, 1), (34u64 * (28 + 29)).div_ceil(28));
        // Splitting a replay changes nothing but the rounding.
        let whole = cost.replay_ms_v1(0, 4_096);
        let halves = cost.replay_ms_v1(0, 2_048) + cost.replay_ms_v1(2_048, 2_048);
        assert!(whole.abs_diff(halves) <= 1, "{whole} against {halves}");
        // Later is dearer, wider is dearer.
        assert!(cost.replay_ms_v1(1 << 20, 1_024) > cost.replay_ms_v1(0, 1_024));
        assert!(cost.replay_ms_v1(0, 1_025) > cost.replay_ms_v1(0, 1_024));
        // A rate that fits nothing saturates rather than wrapping into a budget.
        let never = PalwHeldReplayCostV1 { ms_per_position: u64::MAX, knee_positions: 1 };
        assert_eq!(never.replay_ms_v1(u64::MAX, u64::MAX), u64::MAX);
    }

    /// **The knee is the geometry's**: 28 rows on the context vectors' thinnest row (ADR-0110), and
    /// 15,232 on the dense tier's real one (Qwen2.5-1.5B: 46,792,704 multiply-accumulates of its own
    /// a layer against 3,072 a row of history). With no attention layer it is `u64::MAX`.
    #[test]
    fn the_knee_is_read_off_the_registered_geometry() {
        use crate::palw_qwen25_profile::{
            PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_profile_v7,
        };
        let dense = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 1 << 21, ..QWEN25_1_5B }).unwrap();
        assert_eq!(palw_held_attention_knee_v1(&dense), 15_232);
        let thin = qwen25_a16_profile_v7(PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 4_096,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        })
        .unwrap();
        assert_eq!(palw_held_attention_knee_v1(&thin), 28);
        let mut no_attention = thin.clone();
        no_attention.attn_heads = 0;
        assert_eq!(palw_held_attention_knee_v1(&no_attention), u64::MAX);
    }

    /// **The route prices the whole prefix with its attention** (§10.6). On the dense tier's real
    /// row and testnet-11's 600-DAA window, the line put the boundary at 1,058,823 positions; the
    /// history puts it near 165,000. The linear bound itself now resumes.
    #[test]
    fn the_route_prices_the_whole_prefix_with_its_attention() {
        let cost = PalwHeldReplayCostV1 { ms_per_position: 34, knee_positions: 15_232 };
        let budget = palw_held_seat_budget_ms_v1(600);
        assert_eq!(budget, 36_000_000);
        assert_eq!(palw_held_seat_route_v1(1 << 17, cost, 600), PalwHeldSeatRouteV1::Recompute);
        assert_eq!(palw_held_seat_route_v1(196_608, cost, 600), PalwHeldSeatRouteV1::Resume);
        assert_eq!(palw_held_seat_route_v1(1_058_823, cost, 600), PalwHeldSeatRouteV1::Resume, "the line's own bound");
        let line = PalwHeldReplayCostV1 { knee_positions: u64::MAX, ..cost };
        assert_eq!(palw_held_seat_route_v1(1_058_823, line, 600), PalwHeldSeatRouteV1::Recompute, "which the line admitted");
        // The boundary, found: the widest context that still recomputes.
        let (mut lo, mut hi) = (1u32, 1 << 21);
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if palw_held_seat_route_v1(mid, cost, 600) == PalwHeldSeatRouteV1::Recompute { lo = mid } else { hi = mid }
        }
        assert!((160_000..170_000).contains(&lo), "the boundary moved to {lo}");
    }

    /// **The width's clock is the last interval's, history included.** The dense row at 2M keeps
    /// the wire's 2,048 positions — its last interval replays in 2.7 h of the 10 — and a fetch
    /// that leaves less than that halves it.
    #[test]
    fn the_width_clock_prices_the_last_interval_against_its_history() {
        let cost = PalwHeldReplayCostV1 { ms_per_position: 34, knee_positions: 15_232 };
        let n = 1u32 << 21;
        let p = palw_held_seat_interval_positions_v1(n, cost, 0, 600, 103_008, PALW_HELD_SEAT_INTERVAL_OPENING_CAP_BYTES_V1);
        assert_eq!(p, 2_048, "the wire binds, and the clock admits it");
        let last = cost.replay_ms_v1(u64::from(n - p), u64::from(p));
        assert!((9_000_000..10_000_000).contains(&last), "2.7 h, not the line's 70 s: {last} ms");
        let budget = palw_held_seat_budget_ms_v1(600);
        let tight = palw_held_seat_interval_positions_v1(
            n,
            cost,
            budget - last + 1,
            600,
            103_008,
            PALW_HELD_SEAT_INTERVAL_OPENING_CAP_BYTES_V1,
        );
        assert_eq!(tight, 1_024, "one millisecond short of the last interval's replay halves the width");
    }

    /// **The class's width is the wire's, or the draw's**: the dense held row at 2M opens 2,048
    /// positions an interval (103,008 leaves a position against 4 MiB of digests); a narrow context
    /// is cut into the draw's four; the width is a power of two.
    #[test]
    fn the_class_width_is_the_wires_and_never_past_the_context() {
        use crate::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
        let at = |n_ctx: u32| {
            palw_held_interval_positions_v1(
                &qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).unwrap(),
            )
        };
        assert_eq!(at(1 << 21), 2_048, "the wire binds at 2M");
        assert_eq!(at(512), 128, "the draw's k binds a narrow context: four intervals cover it");
        assert!(at(1 << 21).is_power_of_two());
    }
}
