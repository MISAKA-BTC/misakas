//! **Lane G14-L (user decision 2026-10-11, option (a)): parallel reserved sessions and the k-ary authenticated descent on the legacy
//! held route** — behind the two dormant, never-armed legacy fences together (`palw_legacy_public_filer_v1`: reserved sessions exist
//! only there; `palw_legacy_held_da_v2`: tag 157 exists only there). No new wire type, tag, table or registry number: a reserver's
//! further tag-157 demands JOIN its open reserved session as LANES. Design: `docs/design/palw/g14-completion-matrix.md` §2h.
//!
//! **Why.** G14-B derived (§2c) that a stalling producer answers every reserved session on the last DAA of its `W_disclose` (1,200 on
//! testnet-12, kept: SA-3 asks `W_disclose ≥ 2 ×` finality), and the claim's hard deadline (`trace_retention − W_disclose`, 4,200 after
//! acceptance) lets at most FOUR sessions open one after another. The one-unit-per-session descent needs eleven at 8k. The user chose
//! to keep SA-3 and the liability / lifetime ceilings and to parallelize instead: one search ROUND demands several units at once.
//!
//! **The consensus rule (the fold, `open_da_session_legacy_held_v2`).** Where both fences are in force, a tag-157 demand by a bond that
//! (a) holds a live reservation on the claim, (b) is not a seat of its current panel and (c) already has its reserved session open on
//! the claim JOINS that session as one more lane, instead of being refused as a second session (DA-1), when
//! [`palw_legacy_lane_join_check_v1`] admits it:
//! * every unit of the session, and the new one, is a legacy held unit (`LegacyHeldV2`) — never an event or a reacquisition session;
//! * the join lands within [`PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1`] of the session's opening;
//! * the session holds at most [`PALW_LEGACY_LANES_PER_SESSION_V1`] lanes;
//! * the reservation has joined at most [`PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1`] lanes over its life (its own budget — no
//!   claim-wide or global cap another bond could fill, C6/C8); the session still counts ONE of the reservation's 34 sessions;
//! * the lane pays its own deposit: DA-6's price at the claim's stage, added to the session's exposure on the joiner's free half (A-6),
//!   refunded by a conviction or a default, held and burned at retirement when every lane was answered and nothing was proved;
//! * the deadline counts G14-B's chunk rule: `max(deadline, now + W_disclose + (lanes − 1) × c)` ([`palw_legacy_lane_deadline_v1`],
//!   `c` = [`PALW_LEGACY_LANE_CARRIAGE_DAA_V1`]: one held answer is at most the court's close ceiling, under one block's transaction
//!   bytes), so a producer stalling to the end can still carry every lane's answer; and it never passes the claim's retention.
//!
//! The producer answers each lane by its own tag-158 answer (DA-4: an answer answers every session demanding the unit); the session is
//! refuted when every lane is answered, and DEFAULTS (DA-7) at its deadline if any lane is not.
//!
//! **The k-ary descent (node policy, [`palw_legacy_kary_next_v1`]).** A round at a focus node `(L, i)` the verifier knows to differ
//! demands the `2^s` nodes under it at level `L − s`; each answer authenticates its node to the claim's root and carries its frontier
//! nine levels lower, so one round descends `s + 9` levels. Soundness is G14-V's rule: only nodes FOLDED from authenticated frontiers
//! (or the binding's root) are compared with the verifier's own nodes, from the focus down, always into the leftmost differing child —
//! a matching subtree root covers its whole subtree (collision resistance), a lone matching midpoint is never trusted, and a lie
//! followed by correct values is still found. The width `s` is a pure function of the focus level, the input lanes the root round
//! carries and the session width ([`palw_legacy_kary_width_bits_v1`]), so a restarted verifier re-derives the same round from chain
//! answers alone (G14-R's rebuild, no carried state).
//!
//! **The plan and the refusal by name ([`palw_legacy_parallel_plan_v1`], [`palw_legacy_parallel_feasible_v1`]).** Sessions: the binding
//! read; the root round (the input chunks a fresh verifier acquires ride it as lanes); the further descent rounds; the terminal (the
//! fused leaf's committed witness). Each round is scheduled against the claim's lattice windows by
//! `misaka_palw_kernel::budget::parallel_session_schedule_v1`; a profile no round plan fits is refused as `PARALLEL_SESSIONS: …`.

use crate::Hash64;
use crate::palw_legacy_held_da_v2::{
    PALW_LEGACY_HELD_NODE_DEPTH_V2, PALW_LEGACY_PROMPT_CHUNK_IDS_V2, PalwLegacyFrontierV2, PalwLegacyHeldUnitV2, PalwLegacyOwnTreeV2,
    PalwLegacyTreeV2,
};
use crate::palw_tir_court_v1::{palw_tir_step_tree_height_v1, palw_tir_step_tree_width_v1};
use std::collections::BTreeMap;

/// G14-B / G14-L's schedules, for the node and its tests (which reach the kernel through this crate).
pub use misaka_palw_kernel::budget::{
    ParallelSessionClockV1, ParallelSessionScheduleV1, SessionClockV1, parallel_session_max_replay_v1, parallel_session_schedule_v1,
};

// ---- the terms (consensus constants of the two dormant fences) -------------------------------------------------------------------

/// **Lanes one reserved session holds at most** (`K`). 64 node answers of ≈ 33 KiB each are ≈ 2 MiB a round — 21 DAA of whole blocks at
/// one held answer a block, far inside one `W_disclose`. Enough for the canonical 8k row (2^27 leaves) with two levels to spare.
pub const PALW_LEGACY_LANES_PER_SESSION_V1: u32 = 64;
/// **Lanes beyond each session's first one reservation may join over its life.** Two full rounds (`2 × (K − 1)` = 126) fit; it is the
/// reservation's own budget, beside its 34 sessions.
pub const PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1: u16 = 128;
/// **DAA after a session opens within which a lane may join it.** A node carries one priority carrier a DAA (its funding chains through
/// each carrier's change), so `K` lanes need `K` DAA; twice that is the slack for one lost carrier's resend.
pub const PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1: u64 = 2 * PALW_LEGACY_LANES_PER_SESSION_V1 as u64;
/// **The response window's carriage term per further lane** (G14-B's chunk rule): a held answer is at most the court's close ceiling
/// (≤ one chunk, 100,000 B), under one block's 125,000 transaction bytes — one answer a block, one block a DAA
/// (`misaka_palw_kernel::budget::ROUTE_DAA_PER_CHUNK_V1`).
pub const PALW_LEGACY_LANE_CARRIAGE_DAA_V1: u64 = misaka_palw_kernel::budget::ROUTE_DAA_PER_CHUNK_V1;

/// **Is the parallel-lane rule in force?** Both legacy fences (each dormant and refused when armed on every network).
pub fn palw_legacy_parallel_active_v1(public_filer_active: bool, held_da_v2_active: bool) -> bool {
    public_filer_active && held_da_v2_active
}

/// **A joined session's deadline**: `max(current, now + W_disclose + (lanes_after − 1) × c)` — the stalling producer keeps a full
/// `W_disclose` to react after the last join plus one DAA of carriage for every further lane it must answer. `None` on overflow.
pub fn palw_legacy_lane_deadline_v1(current: u64, now: u64, w_disclose: u64, lanes_after: u32) -> Option<u64> {
    let carriage = u64::from(lanes_after.saturating_sub(1)).checked_mul(PALW_LEGACY_LANE_CARRIAGE_DAA_V1)?;
    Some(current.max(now.checked_add(w_disclose)?.checked_add(carriage)?))
}

/// The latest deadline a lanes session can carry: opened, the join window, `W_disclose` and `K − 1` lanes of carriage (the
/// consistency check's bound).
pub fn palw_legacy_lane_deadline_bound_v1(opened: u64, w_disclose: u64) -> u64 {
    opened
        .saturating_add(PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1)
        .saturating_add(w_disclose)
        .saturating_add(u64::from(PALW_LEGACY_LANES_PER_SESSION_V1 - 1) * PALW_LEGACY_LANE_CARRIAGE_DAA_V1)
}

/// Why a lane may not join (the fold reports it by name).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwLegacyLaneRefusalV1 {
    NotHeldSession,
    Reacquisition,
    DuplicateUnit,
    JoinWindowClosed,
    SessionFull,
    ReservationLanesExhausted,
    OpenedBeforeReservation,
}

impl PalwLegacyLaneRefusalV1 {
    pub fn why(self) -> &'static str {
        match self {
            Self::NotHeldSession => "a lane joins only a session whose every unit is a legacy held unit",
            Self::Reacquisition => "a reacquisition session carries its one unit",
            Self::DuplicateUnit => "the unit is already a lane of the session",
            Self::JoinWindowClosed => "the session's lane-join window has closed",
            Self::SessionFull => "the session holds its 64 lanes",
            Self::ReservationLanesExhausted => "the reservation has joined its 128 extra lanes",
            Self::OpenedBeforeReservation => "the open session predates the bond's reservation: it is not a reserved session",
        }
    }
}

/// **The join rule's pure half** (the fold checks the rest: the gates DA-8 asks of any session, DA-6's price on the free half, the
/// unit not yet answered, the deadline inside retention). Returns the lanes the session holds after the join.
pub fn palw_legacy_lane_join_check_v1(
    session: &crate::palw_da_rcore_v1::PalwDaSessionV1,
    unit: &crate::palw_da_rcore_v1::PalwDaUnitV1,
    reservation: &crate::palw_legacy_public_filer_v1::PalwDisputeReservationRowV1,
    reacquiring: bool,
    now: u64,
) -> Result<u32, PalwLegacyLaneRefusalV1> {
    use crate::palw_da_rcore_v1::PalwDaUnitV1 as U;
    if reacquiring {
        return Err(PalwLegacyLaneRefusalV1::Reacquisition);
    }
    if !matches!(unit, U::LegacyHeldV2(_)) || !session.units.iter().all(|u| matches!(u, U::LegacyHeldV2(_))) {
        return Err(PalwLegacyLaneRefusalV1::NotHeldSession);
    }
    if !session.accuser_is_seat || reservation.reserved_daa > session.opened_daa {
        return Err(PalwLegacyLaneRefusalV1::OpenedBeforeReservation);
    }
    if session.units.contains(unit) {
        return Err(PalwLegacyLaneRefusalV1::DuplicateUnit);
    }
    if now > session.opened_daa.saturating_add(PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1) {
        return Err(PalwLegacyLaneRefusalV1::JoinWindowClosed);
    }
    let lanes_after = u32::try_from(session.units.len()).unwrap_or(u32::MAX).saturating_add(1);
    if lanes_after > PALW_LEGACY_LANES_PER_SESSION_V1 {
        return Err(PalwLegacyLaneRefusalV1::SessionFull);
    }
    if reservation.lanes_joined >= PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1 {
        return Err(PalwLegacyLaneRefusalV1::ReservationLanesExhausted);
    }
    Ok(lanes_after)
}

// ---- the round width and the plan ----------------------------------------------------------------------------------------------

fn floor_log2(x: u32) -> u8 {
    (31 - x.max(1).leading_zeros()) as u8
}

/// **The round's width `s` at a focus of level `level`** (the round demands the `2^s` nodes under the focus at level `level − s`):
/// the fewest rounds to the leaves at `depth` levels a frontier, the root round sharing its session with `input_lanes` input lanes, `K`
/// lanes a session; the extra levels spread evenly over those rounds (each round's width at most what fits). Pure: a restarted
/// verifier at the same focus derives the same round. `Err` names why no width fits.
pub fn palw_legacy_kary_width_bits_at_depth_v1(level: u8, input_lanes: u32, k_max: u32, depth: u8) -> Result<u8, &'static str> {
    if level == 0 {
        return Ok(0);
    }
    if depth == 0 {
        return Err("a frontier covers at least one level");
    }
    if input_lanes >= k_max {
        return Err("the input's lanes leave no lane of the root session for the descent");
    }
    let s_first = floor_log2(k_max - input_lanes);
    let s_rest = floor_log2(k_max);
    let (l, d) = (u32::from(level), u32::from(depth));
    let mut rounds = 1u32;
    let mut reach = d + u32::from(s_first);
    while reach < l {
        reach += d + u32::from(s_rest);
        rounds += 1;
    }
    let extra = l.saturating_sub(d * rounds);
    let s = extra.div_ceil(rounds).min(u32::from(s_first)).min(l - 1);
    Ok(s as u8)
}

/// [`palw_legacy_kary_width_bits_at_depth_v1`] at the legacy node depth (9) and the consensus session width (64).
pub fn palw_legacy_kary_width_bits_v1(level: u8, input_lanes: u32) -> Result<u8, &'static str> {
    palw_legacy_kary_width_bits_at_depth_v1(level, input_lanes, PALW_LEGACY_LANES_PER_SESSION_V1, PALW_LEGACY_HELD_NODE_DEPTH_V2)
}

/// **The input lanes of a claim's root round**: the `PromptIds` chunks a fresh verifier must acquire before its replay — one unit for a
/// flat input, else `⌈prefill / 2,048⌉`. Derived from the binding alone (whether or not THIS verifier needs them), so every verifier
/// derives the same root round.
pub fn palw_legacy_input_lanes_v1(binding: &crate::palw_step_leg::PalwStepBindingV2, form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> u32 {
    use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, palw_prompt_ids_form_of_class_v1};
    let prefill = u64::from(binding.job_context.declared_prefill_tokens);
    if prefill == 0 {
        return 0;
    }
    if palw_prompt_ids_form_of_class_v1(form, &binding.shape_profile) == PalwPromptIdsFormV1::Flat {
        return 1;
    }
    u32::try_from(palw_legacy_input_units_v1(prefill)).unwrap_or(u32::MAX)
}

/// Input chunks of `prefill` ids in the chunked form.
pub fn palw_legacy_input_units_v1(prefill: u64) -> u64 {
    prefill.div_ceil(PALW_LEGACY_PROMPT_CHUNK_IDS_V2)
}

/// **The worst-case round plan of a legacy held pursuit.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwLegacyParallelPlanV1 {
    pub height: u8,
    pub input_lanes: u32,
    /// Lanes of every reserved session in order: the binding read (1), the root round (input + `2^s`), the further rounds, the
    /// terminal (1). Each is ONE of the reservation's 34 sessions.
    pub sessions: Vec<u32>,
    /// The width `s` of each descent round.
    pub width_bits: Vec<u8>,
    /// The index in `sessions` of the round that carries the input (the verifier replays after it).
    pub input_session: Option<usize>,
    /// Node lanes over the whole descent (the frontiers a verifier holds at most).
    pub node_lanes: u32,
    /// Lanes beyond each session's first (the reservation's lane budget).
    pub extra_lanes: u32,
}

/// **Plan the descent at `depth` levels a frontier and `k_max` lanes a session** (every focus subtree full: the worst case).
pub fn palw_legacy_parallel_plan_at_v1(
    leaf_count: u64,
    input_lanes: u32,
    k_max: u32,
    depth: u8,
) -> Result<PalwLegacyParallelPlanV1, String> {
    let height = palw_tir_step_tree_height_v1(leaf_count);
    let mut sessions = vec![1u32];
    let mut width_bits = Vec::new();
    let mut node_lanes = 0u32;
    let mut level = height;
    let mut first = true;
    if input_lanes > k_max {
        return Err(format!(
            "PARALLEL_SESSIONS: the input's {input_lanes} chunks exceed one session's {k_max} lanes (it must be acquired before the replay, \
             in the root round)"
        ));
    }
    while level > 0 {
        let input = if first { input_lanes } else { 0 };
        let s = palw_legacy_kary_width_bits_at_depth_v1(level, input, k_max, depth).map_err(|why| format!("PARALLEL_SESSIONS: {why}"))?;
        let nodes = 1u32 << s;
        sessions.push(input + nodes);
        width_bits.push(s);
        node_lanes = node_lanes.saturating_add(nodes);
        level = (level - s).saturating_sub(depth);
        first = false;
    }
    if first && input_lanes > 0 {
        sessions.push(input_lanes);
    }
    let input_session = (input_lanes > 0).then_some(1);
    sessions.push(1);
    let extra_lanes = sessions.iter().map(|lanes| lanes - 1).sum();
    Ok(PalwLegacyParallelPlanV1 { height, input_lanes, sessions, width_bits, input_session, node_lanes, extra_lanes })
}

/// [`palw_legacy_parallel_plan_at_v1`] at the consensus terms, refusing by name what the reservation's budgets cannot carry.
pub fn palw_legacy_parallel_plan_v1(leaf_count: u64, input_lanes: u32) -> Result<PalwLegacyParallelPlanV1, String> {
    let plan = palw_legacy_parallel_plan_at_v1(leaf_count, input_lanes, PALW_LEGACY_LANES_PER_SESSION_V1, PALW_LEGACY_HELD_NODE_DEPTH_V2)?;
    let sessions_cap = usize::from(crate::palw_legacy_public_filer_v1::PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1);
    if plan.sessions.len() > sessions_cap {
        return Err(format!("PARALLEL_SESSIONS: {} sessions exceed the reservation's {sessions_cap}", plan.sessions.len()));
    }
    if plan.extra_lanes > u32::from(PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1) {
        return Err(format!(
            "PARALLEL_SESSIONS: {} extra lanes exceed the reservation's {PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1}",
            plan.extra_lanes
        ));
    }
    Ok(plan)
}

/// **The parallel pursuit's clock** from a claim's lattice windows (DAA after acceptance), exactly G14-B's session clock
/// (`crate::palw_legacy_public_filer_v1::palw_legacy_pursuit_clock_v1`) plus the lane terms: the verifier's one carrier a DAA, the
/// consensus carriage term `c`, the join window, and the verifier's own replay after the input round (declared, unmeasured).
pub fn palw_legacy_parallel_clock_v1(
    session: misaka_palw_kernel::budget::SessionClockV1,
    replay_daa: u64,
) -> misaka_palw_kernel::budget::ParallelSessionClockV1 {
    misaka_palw_kernel::budget::ParallelSessionClockV1 {
        base: session,
        join_daa_per_lane: 1,
        answer_daa_per_lane: PALW_LEGACY_LANE_CARRIAGE_DAA_V1,
        join_window_daa: PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1,
        replay_daa,
    }
}

/// **Can a legacy held pursuit of `leaf_count` step leaves and `input_units` input chunks FINISH?** The plan at the consensus terms,
/// scheduled against `clock`; `Err` names why not (`PARALLEL_SESSIONS: …`).
pub fn palw_legacy_parallel_feasible_v1(
    leaf_count: u64,
    input_units: u32,
    clock: &misaka_palw_kernel::budget::ParallelSessionClockV1,
) -> Result<(PalwLegacyParallelPlanV1, misaka_palw_kernel::budget::ParallelSessionScheduleV1), String> {
    let plan = palw_legacy_parallel_plan_v1(leaf_count, input_units)?;
    let schedule = misaka_palw_kernel::budget::parallel_session_schedule_v1(clock, &plan.sessions, plan.input_session);
    if !schedule.fits {
        return Err(format!(
            "PARALLEL_SESSIONS: the worst-case rounds {:?} do not finish inside the claim's windows (last open {}, filed {}, open span {}, \
             close span {})",
            plan.sessions, schedule.last_open_daa, schedule.filed_daa, clock.base.open_span_daa, clock.base.close_span_daa
        ));
    }
    Ok((plan, schedule))
}

// ---- the k-ary descent (node policy) -------------------------------------------------------------------------------------------

/// **What the k-ary descent asks next.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwLegacyKaryStepV1 {
    /// The claim's root is the verifier's own.
    Agrees,
    /// One round: these units, each a lane of one reserved session (those already answered are read, not demanded).
    Round { focus_level: u8, focus_index: u64, units: Vec<PalwLegacyHeldUnitV2> },
    /// The first leaf whose committed node is not the verifier's own.
    FirstDivergentLeaf(u64),
    /// Authenticated children agree with the verifier's own under a node that does not: its own replica is inconsistent.
    OwnTreeInconsistent { level: u8, index: u64 },
    /// The verifier cannot produce its own node here.
    OwnTreeUnavailable { level: u8, index: u64 },
    /// Two authenticated answers disagree about one node (impossible for answers the fold accepted: a hash collision or a caller bug).
    Conflict { level: u8, index: u64 },
    /// No width fits this focus (`palw_legacy_kary_width_bits_v1`'s reason).
    Refused(&'static str),
}

/// **The claim's tree as far as authenticated material reaches it**: every frontier's nodes, everything folded from them within each
/// answered subtree, then every node whose children are both known (or whose only child is promoted) — across lanes.
pub struct PalwLegacyKnownTreeV1 {
    leaf_count: u64,
    nodes: BTreeMap<(u8, u64), Hash64>,
    conflict: Option<(u8, u64)>,
}

impl PalwLegacyKnownTreeV1 {
    pub fn build(leaf_count: u64, claim_root: &Hash64, answered: &[PalwLegacyFrontierV2]) -> Self {
        let mut tree = Self { leaf_count, nodes: BTreeMap::new(), conflict: None };
        let height = palw_tir_step_tree_height_v1(leaf_count);
        tree.put(height, 0, *claim_root);
        let mut lowest = height;
        for f in answered {
            for (k, node) in f.nodes.iter().enumerate() {
                tree.put(f.below, f.first + k as u64, *node);
            }
            lowest = lowest.min(f.below);
        }
        // Fold upward level by level: a parent is known when both children are (or its one child is promoted).
        for level in lowest..height {
            let Some(width) = palw_tir_step_tree_width_v1(leaf_count, level) else { break };
            let at: Vec<u64> = tree.nodes.range((level, 0)..=(level, u64::MAX)).map(|((_, i), _)| *i).collect();
            for i in at {
                let parent = i / 2;
                let (left, right) = (2 * parent, 2 * parent + 1);
                let value = if right >= width {
                    tree.nodes.get(&(level, left)).copied()
                } else {
                    match (tree.nodes.get(&(level, left)), tree.nodes.get(&(level, right))) {
                        (Some(l), Some(r)) => Some(crate::palw_step_leg::step_merkle_node_v1(l, r)),
                        _ => None,
                    }
                };
                if let Some(value) = value {
                    tree.put(level + 1, parent, value);
                }
            }
        }
        tree
    }

    fn put(&mut self, level: u8, index: u64, value: Hash64) {
        match self.nodes.get(&(level, index)) {
            Some(held) if *held != value => {
                self.conflict.get_or_insert((level, index));
            }
            Some(_) => {}
            None => {
                self.nodes.insert((level, index), value);
            }
        }
    }

    /// Node `(level, index)` of the CLAIM's tree, where authenticated material reaches it.
    pub fn node(&self, level: u8, index: u64) -> Option<Hash64> {
        self.nodes.get(&(level, index)).copied()
    }

    pub fn leaf_count(&self) -> u64 {
        self.leaf_count
    }
}

/// **The round's units at a focus**: the nodes under `(focus_level, focus_index)` at level `focus_level − s` that exist in the tree.
pub fn palw_legacy_kary_round_units_v1(
    tree: PalwLegacyTreeV2,
    leaf_count: u64,
    focus_level: u8,
    focus_index: u64,
    s: u8,
) -> Vec<PalwLegacyHeldUnitV2> {
    let s = s.min(focus_level.saturating_sub(1));
    let level = focus_level - s;
    let Some(width) = palw_tir_step_tree_width_v1(leaf_count, level) else { return Vec::new() };
    let lo = focus_index << s;
    let hi = ((focus_index + 1) << s).min(width);
    (lo..hi).map(|j| tree.node_unit(level, j)).collect()
}

/// **The k-ary descent at `depth` levels a frontier and `k_max` lanes a session.** Walks the known tree from the root into the
/// leftmost child whose AUTHENTICATED node differs from the verifier's own; stops where a child is not known yet and names that
/// focus's round. `root_input_lanes` are the input lanes the root round shares its session with.
#[allow(clippy::too_many_arguments)]
pub fn palw_legacy_kary_next_at_v1(
    tree: PalwLegacyTreeV2,
    leaf_count: u64,
    claim_root: &Hash64,
    own: &dyn PalwLegacyOwnTreeV2,
    answered: &[PalwLegacyFrontierV2],
    root_input_lanes: u32,
    k_max: u32,
    depth: u8,
) -> PalwLegacyKaryStepV1 {
    let height = palw_tir_step_tree_height_v1(leaf_count);
    let Some(own_root) = own.own_node(height, 0) else {
        return PalwLegacyKaryStepV1::OwnTreeUnavailable { level: height, index: 0 };
    };
    if own_root == *claim_root {
        return PalwLegacyKaryStepV1::Agrees;
    }
    if height == 0 {
        return PalwLegacyKaryStepV1::FirstDivergentLeaf(0);
    }
    let known = PalwLegacyKnownTreeV1::build(leaf_count, claim_root, answered);
    if let Some((level, index)) = known.conflict {
        return PalwLegacyKaryStepV1::Conflict { level, index };
    }
    let (mut level, mut index) = (height, 0u64);
    loop {
        if level == 0 {
            return PalwLegacyKaryStepV1::FirstDivergentLeaf(index);
        }
        let child = level - 1;
        let Some(width) = palw_tir_step_tree_width_v1(leaf_count, child) else {
            return PalwLegacyKaryStepV1::OwnTreeUnavailable { level: child, index: 2 * index };
        };
        let (left, right) = (2 * index, 2 * index + 1);
        if right >= width {
            // A promoted node: its one child IS it (the same hash on both sides).
            index = left;
            level = child;
            continue;
        }
        let (Some(kl), Some(kr)) = (known.node(child, left), known.node(child, right)) else {
            let input = if level == height { root_input_lanes } else { 0 };
            return match palw_legacy_kary_width_bits_at_depth_v1(level, input, k_max, depth) {
                Ok(s) => PalwLegacyKaryStepV1::Round {
                    focus_level: level,
                    focus_index: index,
                    units: palw_legacy_kary_round_units_v1(tree, leaf_count, level, index, s),
                },
                Err(why) => PalwLegacyKaryStepV1::Refused(why),
            };
        };
        match own.own_node(child, left) {
            None => return PalwLegacyKaryStepV1::OwnTreeUnavailable { level: child, index: left },
            Some(mine) if mine != kl => {
                (level, index) = (child, left);
                continue;
            }
            Some(_) => {}
        }
        match own.own_node(child, right) {
            None => return PalwLegacyKaryStepV1::OwnTreeUnavailable { level: child, index: right },
            Some(mine) if mine != kr => (level, index) = (child, right),
            Some(_) => return PalwLegacyKaryStepV1::OwnTreeInconsistent { level, index },
        }
    }
}

/// [`palw_legacy_kary_next_at_v1`] at the legacy node depth (9) and the consensus session width (64).
pub fn palw_legacy_kary_next_v1(
    tree: PalwLegacyTreeV2,
    leaf_count: u64,
    claim_root: &Hash64,
    own: &dyn PalwLegacyOwnTreeV2,
    answered: &[PalwLegacyFrontierV2],
    root_input_lanes: u32,
) -> PalwLegacyKaryStepV1 {
    palw_legacy_kary_next_at_v1(
        tree,
        leaf_count,
        claim_root,
        own,
        answered,
        root_input_lanes,
        PALW_LEGACY_LANES_PER_SESSION_V1,
        PALW_LEGACY_HELD_NODE_DEPTH_V2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_leg::{step_merkle_leaf_v1, step_merkle_node_v1};

    /// A whole tree held in memory (the honest replica of a test, or the producer's committed tree).
    struct Whole {
        levels: Vec<Vec<Hash64>>,
    }

    impl Whole {
        fn of(leaf_hashes: &[Hash64]) -> Self {
            let mut levels = vec![leaf_hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect::<Vec<_>>()];
            while levels.last().unwrap().len() > 1 {
                let last = levels.last().unwrap();
                let mut next = Vec::new();
                let mut pairs = last.chunks_exact(2);
                for p in &mut pairs {
                    next.push(step_merkle_node_v1(&p[0], &p[1]));
                }
                if let [odd] = pairs.remainder() {
                    next.push(*odd);
                }
                levels.push(next);
            }
            Self { levels }
        }
        fn root(&self) -> Hash64 {
            *self.levels.last().unwrap().first().unwrap()
        }
        /// The producer's answer to node `(level, index)` at `depth`: its frontier as tree nodes.
        fn frontier(&self, n: u64, level: u8, index: u64, depth: u8) -> PalwLegacyFrontierV2 {
            let below = level.saturating_sub(depth);
            let shift = u32::from(level - below);
            let width = palw_tir_step_tree_width_v1(n, below).unwrap();
            let lo = index << shift;
            let hi = ((index + 1) << shift).min(width);
            PalwLegacyFrontierV2 {
                level,
                index,
                below,
                first: lo,
                nodes: self.levels[below as usize][lo as usize..hi as usize].to_vec(),
                leaf_hashes: None,
            }
        }
    }

    impl PalwLegacyOwnTreeV2 for Whole {
        fn own_node(&self, level: u8, index: u64) -> Option<Hash64> {
            self.levels.get(level as usize)?.get(index as usize).copied()
        }
    }

    fn honest(n: u64) -> Vec<Hash64> {
        (0..n).map(|i| Hash64::from_u64_word(0x1000 + i)).collect()
    }

    /// Drive the k-ary descent to its end against a producer committed to `committed`; returns the step, the rounds and the lanes.
    fn drive(n: u64, own: &Whole, committed: &Whole, input: u32, k: u32, depth: u8) -> (PalwLegacyKaryStepV1, u32, Vec<u32>) {
        let mut answered = Vec::new();
        let mut rounds = 0u32;
        let mut lanes = Vec::new();
        loop {
            let step = palw_legacy_kary_next_at_v1(PalwLegacyTreeV2::Step, n, &committed.root(), own, &answered, input, k, depth);
            match step {
                PalwLegacyKaryStepV1::Round { units, .. } => {
                    assert!(!units.is_empty() && units.len() as u32 + if rounds == 0 { input } else { 0 } <= k, "{units:?}");
                    rounds += 1;
                    lanes.push(units.len() as u32);
                    for u in units {
                        let (_, level, index) = u.node().unwrap();
                        assert!(level >= 1, "a lane is an interior node");
                        let f = committed.frontier(n, level, index, depth);
                        assert!(!answered.contains(&f), "a unit is never demanded twice");
                        answered.push(f);
                    }
                    assert!(rounds <= 64, "terminates");
                }
                other => return (other, rounds, lanes),
            }
        }
    }

    /// **Soundness, exhaustively on small trees**: a lie at EVERY position (isolated: every later leaf honest), the last leaf, several
    /// lies (the first is found), at frontier depths 1–3 and widths 1–8, every leaf count 1..=70 — the first divergent leaf, always, and
    /// never more rounds than the plan.
    #[test]
    fn the_kary_descent_finds_the_first_lie_at_every_position() {
        for depth in 1u8..=3 {
            for k in [1u32, 2, 4, 8] {
                for n in 1u64..=70 {
                    let leaves = honest(n);
                    let own = Whole::of(&leaves);
                    let plan = palw_legacy_parallel_plan_at_v1(n, 0, k, depth).unwrap();
                    assert_eq!(drive(n, &own, &own, 0, k, depth).0, PalwLegacyKaryStepV1::Agrees);
                    for lie in 0..n {
                        let mut bad = leaves.clone();
                        bad[lie as usize] = Hash64::from_u64_word(0xBAD);
                        let committed = Whole::of(&bad);
                        let (step, rounds, lanes) = drive(n, &own, &committed, 0, k, depth);
                        assert_eq!(step, PalwLegacyKaryStepV1::FirstDivergentLeaf(lie), "n {n} k {k} depth {depth} lie {lie}");
                        assert!(rounds as usize <= plan.width_bits.len(), "n {n} k {k} depth {depth}: {rounds} rounds, plan {plan:?}");
                        assert!(lanes.iter().all(|l| *l <= k), "every round within the session width");                        // Two lies: the first is located, never the second.
                        if lie + 3 < n {
                            let mut two = bad.clone();
                            two[(lie + 3) as usize] = Hash64::from_u64_word(0xBAD2);
                            let (step, ..) = drive(n, &own, &Whole::of(&two), 0, k, depth);
                            assert_eq!(step, PalwLegacyKaryStepV1::FirstDivergentLeaf(lie));
                        }
                    }
                }
            }
        }
    }

    /// At the consensus depth (9): a 2^12 + 37-leaf tree, a lie at a stride of positions and the last leaf, the root round carrying
    /// four input lanes.
    #[test]
    fn the_kary_descent_at_the_legacy_depth_and_the_last_leaf() {
        let n = (1u64 << 12) + 37;
        let leaves = honest(n);
        let own = Whole::of(&leaves);
        for lie in (0..n).step_by(97).chain([n - 1, n - 2, 1023, 1024]) {
            let mut bad = leaves.clone();
            bad[lie as usize] = Hash64::from_u64_word(0xBAD);
            let (step, rounds, lanes) = drive(n, &own, &Whole::of(&bad), 4, PALW_LEGACY_LANES_PER_SESSION_V1, 9);
            assert_eq!(step, PalwLegacyKaryStepV1::FirstDivergentLeaf(lie));
            assert_eq!(rounds, 1, "height 13: one round (13 = 4 + 9)");
            // The round's 2^4 slots at level 9 exist only where the tree does: width ⌈4,133 / 512⌉ = 9.
            assert_eq!(lanes, vec![9]);
        }
    }

    /// A matching midpoint is never trusted: a producer whose committed tree differs only at one leaf, with every authenticated
    /// subtree to its left and right matching, is located there; an own replica that disagrees nowhere under a differing node is
    /// reported inconsistent, never guessed.
    #[test]
    fn an_isolated_lie_after_matching_values_and_an_inconsistent_own_tree() {
        let n = 300u64;
        let leaves = honest(n);
        let own = Whole::of(&leaves);
        let mut bad = leaves.clone();
        bad[217] = Hash64::from_u64_word(0xBAD);
        let committed = Whole::of(&bad);
        let (step, ..) = drive(n, &own, &committed, 0, 4, 2);
        assert_eq!(step, PalwLegacyKaryStepV1::FirstDivergentLeaf(217));
        // An own tree whose root differs but whose children equal the claim's: inconsistent.
        struct Lying<'a>(&'a Whole, Hash64);
        impl PalwLegacyOwnTreeV2 for Lying<'_> {
            fn own_node(&self, level: u8, index: u64) -> Option<Hash64> {
                if level as usize == self.0.levels.len() - 1 { Some(self.1) } else { self.0.own_node(level, index) }
            }
        }
        let liar = Lying(&own, Hash64::from_u64_word(7));
        let height = palw_tir_step_tree_height_v1(n);
        assert_eq!(height, 9);
        // Every node at level 7 (width 3) answered at depth 2: the root's children are known, and equal the liar's own.
        let answered: Vec<_> = (0..3u64).map(|j| own.frontier(n, height - 2, j, 2)).collect();
        let step = palw_legacy_kary_next_at_v1(PalwLegacyTreeV2::Step, n, &own.root(), &liar, &answered, 0, 4, 2);
        assert_eq!(step, PalwLegacyKaryStepV1::OwnTreeInconsistent { level: height, index: 0 });
        // A frontier that contradicts the binding's root is a conflict, never a descent.
        let mut forged = answered.clone();
        forged[0].nodes[0] = Hash64::from_u64_word(0xF0);
        let step = palw_legacy_kary_next_at_v1(PalwLegacyTreeV2::Step, n, &own.root(), &liar, &forged, 0, 4, 2);
        assert!(matches!(step, PalwLegacyKaryStepV1::Conflict { .. }), "{step:?}");
    }

    #[test]
    fn rounds_of_k_and_the_8k_plan() {
        // rounds(k, leaves): every height up to 40 at the legacy depth, k in 1..=64.
        for k in [1u32, 2, 4, 8, 16, 32, 64] {
            let s = floor_log2(k) as u32;
            for h in 1u8..=40 {
                let n = 1u64 << h;
                let plan = palw_legacy_parallel_plan_at_v1(n, 0, k, 9).unwrap();
                let rounds = plan.width_bits.len() as u32;
                assert_eq!(rounds, u32::from(h).div_ceil(9 + s), "k {k} h {h}: ⌈h / (9 + log2 k)⌉ rounds");
                assert!(plan.sessions.iter().all(|l| *l <= k));
            }
        }
        // The canonical 8k row (≈105.5M leaves, height 27) with four input lanes: [binding, 4 + 32, 16, terminal].
        let p = palw_legacy_parallel_plan_v1(105_500_000, 4).unwrap();
        assert_eq!(p.height, 27);
        assert_eq!(p.sessions, vec![1, 36, 16, 1]);
        assert_eq!(p.width_bits, vec![5, 4]);
        assert_eq!((p.node_lanes, p.extra_lanes), (48, 50));
        // Without input lanes: the same two rounds.
        assert_eq!(palw_legacy_parallel_plan_v1(105_500_000, 0).unwrap().sessions, vec![1, 32, 16, 1]);
        // The largest height two descent rounds reach with four input lanes: 29 (5 + 9 + 6 + 9); 30 needs a third.
        assert_eq!(palw_legacy_parallel_plan_v1(1 << 29, 4).unwrap().sessions, vec![1, 36, 64, 1]);
        assert_eq!(palw_legacy_parallel_plan_v1((1 << 29) + 1, 4).unwrap().sessions.len(), 5);
        // 2M: 1,024 input chunks exceed a session.
        let two_m = palw_legacy_parallel_plan_v1(1 << 40, palw_legacy_input_units_v1(2_097_152) as u32).unwrap_err();
        assert!(two_m.starts_with("PARALLEL_SESSIONS: the input's 1024 chunks"), "{two_m}");
        // 2^40 (the ladder top): three descent rounds of 64 do not reach 40 levels in two.
        assert_eq!(palw_legacy_parallel_plan_v1(1 << 40, 4).unwrap().width_bits.len(), 3);
    }

    /// **G14-B's legacy rows, re-derived for parallel sessions** on the testnet-12 lattice (bind 600, receipt 600, challenge 1,200 —
    /// `W_disclose`, unshortened: SA-3 — court 3,000; the verifier's first step 2, turnaround 1, filing 1, G14-B's own values; replay 0).
    #[test]
    fn the_legacy_profile_table_on_the_t12_lattice() {
        use crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1 as W;
        let lattice = crate::palw_legacy_public_filer_v1::palw_legacy_pursuit_clock_v1(
            W.window_bind,
            W.window_receipt,
            W.window_challenge,
            W.window_court,
            2,
            1,
            1,
        );
        assert_eq!((lattice.open_span_daa, lattice.close_span_daa, lattice.session_window_daa), (4_200, 5_400, 1_200));
        let clock = palw_legacy_parallel_clock_v1(lattice, 0);
        // Legacy held 8k (the canonical row: ≈105.5M leaves, height 27; four input chunks): FITS in four sessions.
        let (plan, s) = palw_legacy_parallel_feasible_v1(105_500_000, 4, &clock).unwrap();
        assert_eq!(plan.sessions, vec![1, 36, 16, 1]);
        assert_eq!(s.opens, vec![2, 1_203, 2_474, 3_705]);
        assert_eq!(s.deadlines, vec![1_202, 2_473, 3_704, 4_905]);
        assert_eq!((s.filed_daa, s.open_slack_daa, s.close_slack_daa), (4_906, 495, 494));
        assert_eq!(parallel_session_max_replay_v1(&clock, &plan.sessions, plan.input_session), Some(494));
        // A verifier that already holds the input: the same rounds without the input lanes.
        let (held, sh) = palw_legacy_parallel_feasible_v1(105_500_000, 0, &clock).unwrap();
        assert_eq!((held.sessions, sh.filed_daa), (vec![1, 32, 16, 1], 4_898));
        // The one-unit descent of the same claim: 4 input + 3 node rounds + 2 = 9 sequential sessions, of which 4 fit (G14-B's FAIL).
        let sequential = crate::palw_legacy_public_filer_v1::palw_legacy_pursuit_sessions_v1(8_192, 105_500_000);
        assert_eq!(sequential, 9);
        assert!(!misaka_palw_kernel::budget::session_pursuit_schedule_v1(&lattice, sequential as u32).fits);
        // The boundary: height 29 fits (two rounds, 36 and 64 lanes); height 30 needs a fifth session and is refused by name.
        let (edge, s29) = palw_legacy_parallel_feasible_v1(1 << 29, 4, &clock).unwrap();
        assert_eq!((edge.sessions.clone(), s29.filed_daa), (vec![1, 36, 64, 1], 5_002));
        let past = palw_legacy_parallel_feasible_v1((1 << 29) + 1, 4, &clock).unwrap_err();
        assert!(past.starts_with("PARALLEL_SESSIONS: the worst-case rounds"), "{past}");
        // G14-B's 2^40 ladder top (its stand-in for "8k"): three descent rounds, five sessions — refused by name.
        assert!(palw_legacy_parallel_feasible_v1(1 << 40, 4, &clock).unwrap_err().starts_with("PARALLEL_SESSIONS"));
        // Legacy held 2M: 1,024 input chunks exceed one session's 64 lanes — refused by name, whatever its tree.
        let two_m = palw_legacy_parallel_feasible_v1(1 << 30, palw_legacy_input_units_v1(2_097_152) as u32, &clock).unwrap_err();
        assert!(two_m.starts_with("PARALLEL_SESSIONS: the input's 1024 chunks exceed one session's 64 lanes"), "{two_m}");
        // A slower verifier: the 8k row still fits with up to 494 DAA of own replay after the input round, and not one more.
        let slow = palw_legacy_parallel_clock_v1(lattice, 495);
        assert!(palw_legacy_parallel_feasible_v1(105_500_000, 4, &slow).is_err());
    }

    #[test]
    fn the_lane_deadline_counts_one_carriage_daa_a_lane() {
        assert_eq!(palw_legacy_lane_deadline_v1(1_210, 10, 1_200, 1), Some(1_210));
        assert_eq!(palw_legacy_lane_deadline_v1(1_210, 10, 1_200, 2), Some(1_211));
        assert_eq!(palw_legacy_lane_deadline_v1(1_211, 11, 1_200, 3), Some(1_213), "a later join restarts W_disclose from it");
        // 36 lanes joined one a DAA: the last join + W + 35.
        let mut d = 10 + 1_200;
        for lane in 2..=36u32 {
            d = palw_legacy_lane_deadline_v1(d, 10 + u64::from(lane - 1), 1_200, lane).unwrap();
        }
        assert_eq!(d, 10 + 35 + 1_200 + 35);
        assert!(d <= palw_legacy_lane_deadline_bound_v1(10, 1_200));
        assert_eq!(palw_legacy_lane_deadline_v1(0, u64::MAX, 1, 1), None);
    }
}
