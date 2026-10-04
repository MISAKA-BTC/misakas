//! **Consensus accounting v2** (ADR-0172; the fence `palw_accounting_v2`, DORMANT on every preset): three block tiers — C-BLUE carries
//! the chain, E-BLUE carries a claim's execution and nothing else, FALLBACK is the one transparent reserve — and one tick a slot.
//!
//! **This module is the pure core and nothing reads it yet.** No GHOSTDAG site, no clock site and no fold calls these functions; the
//! fence exists, hashes Some-only, validates its prerequisites, and is `None` everywhere. They are written first, and tested as
//! properties, so that the wiring (spec §3–§5) can only call a rule that has already been shown to hold the invariants:
//!
//! * **I1** `round_count↑ ≠ blue_score↑ ≠ DAA↑` — an E-BLUE member adds nothing to any scoring quantity ([`scoring_delta_v2`]).
//! * **I2** `fallback_count↑` never reddens real work — a REAL candidate that hangs from the merging chain does not count FALLBACK peers
//!   ([`peer_effect_v2`]); one that does not hang is classic (F1: no borrowing across forks, unconditional here).
//! * **I3** per claim, `attempt_weight + Σ round_weight ≤ W_claim` ([`claim_weight_split_v2`], [`claim_weight_bound_holds_v2`]).
//! * **I4** one tick a slot — REAL first, FALLBACK a reserve behind a grace, REAL + FALLBACK in one mergeset is +1 never +2
//!   ([`palw_clock_carrier_v2`]).
//!
//! **R-NoClass.** Nothing here takes a class id. A lane is the PoW algorithm a header satisfies (proved by that PoW, not declared); a class
//! is a claim about state and is never evidence in the declarer's favour. The inputs are the lane, the candidate's own DAA score and
//! the fence (E0), one DAG fact — does the candidate hang from the merging block's selected chain (E1) — and constants (E2). State (E3)
//! decides credit in the fold, never colour.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_ROUND_V1, PALW_ATTEMPT_BLUE_WORK_LOG2, is_palw_attempt_algo_id};

/// **The FALLBACK lane's algorithm id** (ADR-0172 Q1, recommended; 8 is the retired heartbeat, 10 the round lane, 11 RFC-0008's slice).
pub const POW_ALGO_ID_PALW_FALLBACK_V1: u8 = 12;

/// **G — the grace** a FALLBACK waits behind the slot's opening before it may carry the tick (ADR-0172 §7, Q4): the REAL-first window.
/// The same 20 s the heartbeat miner's node policy has used since ADR-0165 (`PALW_REAL_TICK_GRACE_MS_V1`).
pub const PALW_ACCOUNTING_V2_GRACE_MS: u64 = 20_000;

/// **σ — the rounds' share of a claim's weight, in permille** (ADR-0172 §6, Q3). `0` makes rounds weightless: the recommended initial value,
/// with the budget machinery built so a later fence can raise it.
pub const PALW_ACCOUNTING_V2_SIGMA_PERMILLE: u64 = 0;

/// The rule's own version, hashed with the fence so a change to any value here is a new network id.
pub const PALW_ACCOUNTING_V2_VERSION: u64 = 1;

/// **The values the fingerprint hashes beside the fence's height** — `[G_ms, σ_permille, version]`.
pub const fn palw_accounting_v2_value_v1() -> [u64; 3] {
    [PALW_ACCOUNTING_V2_GRACE_MS, PALW_ACCOUNTING_V2_SIGMA_PERMILLE, PALW_ACCOUNTING_V2_VERSION]
}

/// **ε** — a FALLBACK's (and a legacy heartbeat's) blue work, the lane's one unit (ADR-0060 Decision 1.2).
const FALLBACK_BLUE_WORK: u128 = crate::palw_heartbeat_v1::HEARTBEAT_BLUE_WORK_EPSILON as u128;

// ---------------------------------------------------------------------------------------------------------------------------------
// Lanes and colouring (header stage: E0 + E1 + E2)
// ---------------------------------------------------------------------------------------------------------------------------------

/// **What a header's lane is** — a function of the algo id, the header's OWN DAA score and the fence, nothing else. No class is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneV2 {
    /// The attempt lane (algo 6 / 9): C-BLUE candidates.
    Attempt,
    /// The round lane (algo 10): E-BLUE candidates.
    Exec,
    /// The FALLBACK lane (algo 12), only where the fence is in force at the header's own DAA score.
    Fallback,
    /// A heartbeat (algo 8) made BELOW the fence: valid for ever, a yielding peer and a tick source with no grace.
    LegacyHeartbeat,
    /// A heartbeat made AT OR PAST the fence: the lane is retired (invalid; named so a caller refuses it by name).
    Retired,
    /// Any other lane (hash lanes, the receipt lane, an algo-12 header below the fence, unknown ids).
    Other,
}

/// Is the fence in force at `daa_score`? `None` and `never()` are not.
fn armed_at(fence: Option<ForkActivation>, daa_score: u64) -> bool {
    fence.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
}

/// **The lane of a header** (E0). Keyed on the header's own DAA score, the ADR-0105 key: fixed before any block that merges it is
/// coloured, so no verdict depends on the merging block's own output.
pub fn lane_v2(algo_id: u8, daa_score: u64, fence: Option<ForkActivation>) -> LaneV2 {
    let armed = armed_at(fence, daa_score);
    if algo_id == POW_ALGO_ID_HEARTBEAT_V1 {
        return if armed { LaneV2::Retired } else { LaneV2::LegacyHeartbeat };
    }
    if algo_id == POW_ALGO_ID_PALW_FALLBACK_V1 {
        return if armed { LaneV2::Fallback } else { LaneV2::Other };
    }
    if algo_id == POW_ALGO_ID_PALW_ROUND_V1 {
        return LaneV2::Exec;
    }
    if is_palw_attempt_algo_id(algo_id) {
        return LaneV2::Attempt;
    }
    LaneV2::Other
}

/// **How a mergeset candidate is coloured** (ADR-0172 §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColourRuleV2 {
    /// Below the fence: ADR-0105's rule, byte for byte — the caller keeps the code it has.
    Legacy,
    /// The k-cluster rule over every blue. A REAL candidate that does not hang from the merging chain (F1: no borrowing across forks).
    Classic,
    /// A REAL candidate that hangs from the merging chain: FALLBACK (and legacy heartbeat) peers are invisible to it.
    Weighted,
    /// A FALLBACK candidate: counted against every blue, never enlarges a C block's recorded anticone count (today's `Heartbeat` rule).
    Yielding,
    /// An Exec (round) candidate: recorded non-scoring, never walked, never a peer.
    ExecNonScoring,
}

/// **The colour rule of a candidate** — pure, header-stage. `hangs_from_merging_chain` is the one DAG fact (ADR-0105 §11's walk on the
/// GHOSTDAG store); it matters only for the attempt lane, and F1 is UNCONDITIONAL past this fence (no second height to arm).
pub fn colour_rule_v2(lane: LaneV2, candidate_daa: u64, fence: Option<ForkActivation>, hangs_from_merging_chain: bool) -> ColourRuleV2 {
    if !armed_at(fence, candidate_daa) {
        return ColourRuleV2::Legacy;
    }
    match lane {
        LaneV2::Exec => ColourRuleV2::ExecNonScoring,
        LaneV2::Fallback => ColourRuleV2::Yielding,
        LaneV2::Attempt if hangs_from_merging_chain => ColourRuleV2::Weighted,
        // An attempt that hangs from another branch is classic; so is anything the lane map could not place (it is invalid anyway,
        // and the conservative rule is the one that counts it).
        LaneV2::Attempt | LaneV2::LegacyHeartbeat | LaneV2::Retired | LaneV2::Other => ColourRuleV2::Classic,
    }
}

/// **What a blue peer does to a candidate's walk** (ADR-0172 §4.2): whether it is counted against the candidate's anticone, and whether
/// the candidate turning blue enlarges the peer's recorded count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerEffectV2 {
    /// The peer counts in the candidate's blue anticone.
    pub counts: bool,
    /// The peer's recorded anticone count is enlarged by the candidate.
    pub enlarges: bool,
}

fn is_transparent_lane(peer: LaneV2) -> bool {
    matches!(peer, LaneV2::Fallback | LaneV2::LegacyHeartbeat)
}

/// **The peer effect under a rule**, or `None` for [`ColourRuleV2::Legacy`], which the existing code decides. An Exec peer is never in
/// `mergeset_blues`, so it has no effect under any rule.
pub fn peer_effect_v2(rule: ColourRuleV2, peer: LaneV2) -> Option<PeerEffectV2> {
    if peer == LaneV2::Exec {
        return match rule {
            ColourRuleV2::Legacy => None,
            _ => Some(PeerEffectV2 { counts: false, enlarges: false }),
        };
    }
    match rule {
        ColourRuleV2::Legacy => None,
        ColourRuleV2::Classic => Some(PeerEffectV2 { counts: true, enlarges: true }),
        // Invisible: not counted, its count not consulted, not enlarged.
        ColourRuleV2::Weighted if is_transparent_lane(peer) => Some(PeerEffectV2 { counts: false, enlarges: false }),
        ColourRuleV2::Weighted => Some(PeerEffectV2 { counts: true, enlarges: true }),
        // A FALLBACK candidate is counted against every blue (it yields), but it only ever enlarges a transparent-lane peer's count.
        ColourRuleV2::Yielding => Some(PeerEffectV2 { counts: true, enlarges: is_transparent_lane(peer) }),
        ColourRuleV2::ExecNonScoring => Some(PeerEffectV2 { counts: false, enlarges: false }),
    }
}

/// **The E-BLUE structural verdict** (ADR-0172 §4.3): an Exec candidate is E-BLUE iff it hangs from the merging block's own selected chain
/// within the merge-depth window; otherwise it is RED. Recomputed from the DAG, never stored; E-BLUE grants no work, no score, no tick and
/// no k effect, so a header-only verdict is enough — whether it is CREDITED is the fold's (E3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecVerdictV2 {
    EBlue,
    Red,
}

/// `None` for a candidate that is not an Exec-lane block under the fence (the caller keeps its existing colour).
pub fn exec_verdict_v2(lane: LaneV2, candidate_daa: u64, fence: Option<ForkActivation>, hangs_from_merging_chain: bool) -> Option<ExecVerdictV2> {
    if lane != LaneV2::Exec || !armed_at(fence, candidate_daa) {
        return None;
    }
    Some(if hangs_from_merging_chain { ExecVerdictV2::EBlue } else { ExecVerdictV2::Red })
}

/// **What a mergeset adds to the scoring quantities** (I1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScoringDeltaV2 {
    /// Blue members counted into `blue_score`.
    pub blue_score: u64,
    /// Their blue work (attempt constant `2^20`; ε for FALLBACK and legacy heartbeats; 0 for Exec).
    pub blue_work: u128,
    /// Blue members that occupy `k` anticone budget.
    pub k_members: u64,
}

/// **The scoring delta of a set of coloured members** — `(lane, blue)`. An Exec member contributes nothing whatever its flag: it consumes
/// no `k`, no blue score, no blue work. `round_count↑` therefore moves none of the three (I1).
pub fn scoring_delta_v2(members: &[(LaneV2, bool)]) -> ScoringDeltaV2 {
    let mut delta = ScoringDeltaV2::default();
    for &(lane, blue) in members {
        if !blue {
            continue;
        }
        let work = match lane {
            LaneV2::Attempt => 1u128 << PALW_ATTEMPT_BLUE_WORK_LOG2,
            LaneV2::Fallback | LaneV2::LegacyHeartbeat => FALLBACK_BLUE_WORK,
            // E-BLUE adds nothing; a retired or foreign lane never reaches a valid mergeset.
            LaneV2::Exec | LaneV2::Retired | LaneV2::Other => continue,
        };
        delta.blue_score += 1;
        delta.blue_work += work;
        delta.k_members += 1;
    }
    delta
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The clock (header stage: facts are mergeset headers)
// ---------------------------------------------------------------------------------------------------------------------------------

/// **What the clock reads of a mergeset** (selected parent included, E-BLUE excluded): header facts only, no state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockFactsV2 {
    /// Blocks `bits` prices (none on a V2 network; a priced block IS the clock and no source stands in).
    pub priced: u64,
    /// Attempt-lane sources, and the newest stamp among them.
    pub real: u64,
    pub newest_real_ms: Option<u64>,
    /// FALLBACK sources (algo 12), and the newest stamp.
    pub fallback: u64,
    pub newest_fallback_ms: Option<u64>,
    /// Legacy heartbeats (algo 8 made below the fence): a FALLBACK-kind source with no grace.
    pub legacy_beats: u64,
    pub newest_legacy_ms: Option<u64>,
}

/// **Who carries the slot's tick.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierV2 {
    /// No source qualifies: the mergeset does not tick.
    None,
    /// A REAL attempt stamped at or past the slot.
    Real,
    /// REAL did not qualify; a FALLBACK (or legacy heartbeat) did.
    Fallback,
}

impl CarrierV2 {
    /// `granted`: the mergeset removes ONE exemption — DAA +1, never +2 however many sources it holds.
    pub const fn granted(self) -> bool {
        !matches!(self, CarrierV2::None)
    }
}

/// **The tick carrier of a mergeset** (ADR-0172 §7). `slot_ms` is the cursor's next slot (`None`: no cursor, the slot is open).
///
/// ```text
/// real_ok     = priced == 0 ∧ real > 0     ∧ newest_real     ≥ slot
/// fallback_ok = priced == 0 ∧ fallback > 0 ∧ newest_fallback ≥ slot + G     (a legacy heartbeat: ≥ slot, G = 0)
/// carrier     = Real if real_ok, else Fallback if fallback_ok, else None
/// ```
///
/// A function of header facts, so it can be computed when the header's DAA score is fixed (ADR-0142). Both kinds stay valid blocks: nothing
/// is refused because the other exists. With `grace_ms = 0` and no legacy distinction this is ADR-0165's `palw_clock_tick_source_v1`
/// followed by the slot test, byte for byte (tested).
pub fn palw_clock_carrier_v2(facts: &ClockFactsV2, slot_ms: Option<u64>, grace_ms: u64) -> CarrierV2 {
    if facts.priced != 0 {
        return CarrierV2::None;
    }
    let at_or_past = |newest: Option<u64>, floor: u64| newest.is_some_and(|ms| slot_ms.is_none_or(|slot| ms >= slot.saturating_add(floor)));
    let real_ok = facts.real > 0 && at_or_past(facts.newest_real_ms, 0);
    if real_ok {
        return CarrierV2::Real;
    }
    let fallback_ok = (facts.fallback > 0 && at_or_past(facts.newest_fallback_ms, grace_ms))
        || (facts.legacy_beats > 0 && at_or_past(facts.newest_legacy_ms, 0));
    if fallback_ok { CarrierV2::Fallback } else { CarrierV2::None }
}

/// **The header-stage stamp rule of a FALLBACK** (the H3 analogue): below `slot + G` it can never carry, so it is refused — a time rule
/// on the header's own parents' cursor, **not** a rule about whether a REAL block exists.
pub fn fallback_stamp_admits_v2(stamp_ms: u64, slot_ms: Option<u64>, grace_ms: u64) -> bool {
    slot_ms.is_none_or(|slot| stamp_ms >= slot.saturating_add(grace_ms))
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The per-claim weight budget (fold stage: E3)
// ---------------------------------------------------------------------------------------------------------------------------------

/// **How a claim's weight is split** between its attempt and its rounds (ADR-0172 §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaimSplitV2 {
    /// What the claim's weight was split from.
    pub w_claim: u128,
    /// The attempt's weight: `W_claim − pool`.
    pub attempt: u128,
    /// The rounds' pool: `⌊W_claim · σ / 1000⌋`.
    pub pool: u128,
    /// The tickets the claim minted (≥ 0).
    pub n_tickets: u32,
    /// Each credited round's share: `⌊pool / N⌋` (0 when `N = 0`). Shares never granted are never created.
    pub per_round: u128,
}

/// **The split**, or `None` for a σ above 1000 permille or an arithmetic overflow (a hostile input is refused, never a panic).
pub fn claim_weight_split_v2(w_claim: u128, sigma_permille: u16, n_tickets: u32) -> Option<ClaimSplitV2> {
    if sigma_permille > 1000 {
        return None;
    }
    let pool = w_claim.checked_mul(sigma_permille as u128)? / 1000;
    let attempt = w_claim.checked_sub(pool)?;
    let per_round = if n_tickets == 0 { 0 } else { pool / n_tickets as u128 };
    Some(ClaimSplitV2 { w_claim, attempt, pool, n_tickets, per_round })
}

/// **I3: `attempt + Σ round shares ≤ W_claim`** after `rounds_credited` rounds — and no more rounds than tickets.
pub fn claim_weight_bound_holds_v2(split: &ClaimSplitV2, rounds_credited: u32) -> bool {
    if rounds_credited > split.n_tickets {
        return false;
    }
    match split.per_round.checked_mul(rounds_credited as u128).and_then(|rounds| rounds.checked_add(split.attempt)) {
        Some(total) => total <= split.w_claim,
        None => false,
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The fence
// ---------------------------------------------------------------------------------------------------------------------------------

impl Params {
    /// `palw_accounting_v2`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_accounting_v2_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_accounting_v2.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Is consensus accounting v2 in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_accounting_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_accounting_v2_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`] (last, after every fence's own), each naming what is missing:
    /// the fence off a `ConsensusV2` network; and, at or below its height, the floor machine (`palw_floor_reserve_v1`), the real clock tick,
    /// the cursor / floor / lead cap / anchor clock / single lottery it extends, ADR-0105's transparency **and its F1 same-chain
    /// restriction** (F1 is unconditional here, so it must already be a rule), the execution lane, the anchor window and the model registry.
    ///
    /// **Not yet checked (their fields live on other branches):** the mutual exclusion with `palw_exec_class_v1` (ADR-0168),
    /// `palw_ws_clock_v1`, `palw_merge_admission_v1` and `palw_work_slice_v1` (ADR-0169). Integration adds those lines (spec §1).
    pub fn validate_palw_accounting_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_accounting_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()) else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_accounting_v2 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let below = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        let needs: [(bool, &'static str); 11] = [
            (below(self.palw_floor_reserve_v1), "palw_accounting_v2 needs palw_floor_reserve_v1 at or below it: the floor machine is FALLBACK's Idle gate"),
            (below(self.palw_real_clock_tick_v1), "palw_accounting_v2 needs palw_real_clock_tick_v1 at or below it: it replaces that tick-source rule"),
            (below(self.palw_clock_cursor), "palw_accounting_v2 needs palw_clock_cursor at or below it: the carrier rule is asked of the cursor's slot"),
            (below(self.palw_clock_floor), "palw_accounting_v2 needs palw_clock_floor at or below it: H3/H5 bound the rate the carrier rule relies on"),
            (below(self.palw_clock_lead_cap), "palw_accounting_v2 needs palw_clock_lead_cap at or below it: the cap bounds a FALLBACK's stamp"),
            (below(self.palw_anchor_clock), "palw_accounting_v2 needs palw_anchor_clock at or below it: lanes are unpriced there"),
            (below(self.palw_single_lottery), "palw_accounting_v2 needs palw_single_lottery at or below it: an attempt is unpriced by bits only there"),
            (
                below(self.palw_heartbeat_transparent) && below(self.palw_heartbeat_transparent_same_chain),
                "palw_accounting_v2 needs palw_heartbeat_transparent and palw_heartbeat_transparent_same_chain (ADR-0105 F1) at or below it",
            ),
            (below(self.palw_execution_lane.map(|lane| lane.activation)), "palw_accounting_v2 needs palw_execution_lane at or below it: rounds are its blocks"),
            (below(self.palw_anchor_window_v1), "palw_accounting_v2 needs palw_anchor_window_v1 at or below it: only REAL attempts feed the window past the fence"),
            (below(self.palw_model_registry), "palw_accounting_v2 needs palw_model_registry at or below it: a REAL attempt is a registry class's"),
        ];
        for (ok, message) in needs {
            if !ok {
                return Err(PalwModeV2Error::Invalid(message));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_clock_cursor_v1::{PalwClockCursorV1, palw_clock_slot_admits_v1};
    use crate::palw_real_share_v1::{PalwClockMergesetFactsV1, palw_clock_tick_source_v1};
    use crate::pow_layer0::{POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3};

    const F: u64 = 1_000;
    const I: u64 = crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;
    const G: u64 = PALW_ACCOUNTING_V2_GRACE_MS;

    fn fence() -> Option<ForkActivation> {
        Some(ForkActivation::new(F))
    }

    /// A tiny deterministic generator (no dependency, reproducible failures).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    #[test]
    fn a_lane_is_the_algo_id_and_the_headers_own_daa_never_a_class() {
        for algo in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3] {
            assert_eq!(lane_v2(algo, F - 1, fence()), LaneV2::Attempt);
            assert_eq!(lane_v2(algo, F, fence()), LaneV2::Attempt);
            assert_eq!(lane_v2(algo, F, None), LaneV2::Attempt, "the lane exists whether or not the fence does");
        }
        assert_eq!(lane_v2(POW_ALGO_ID_PALW_ROUND_V1, 5, fence()), LaneV2::Exec);
        // The heartbeat is legacy below the fence and retired from it; FALLBACK exists only from it.
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, F - 1, fence()), LaneV2::LegacyHeartbeat);
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, F, fence()), LaneV2::Retired);
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, u64::MAX, None), LaneV2::LegacyHeartbeat, "dormant: the heartbeat is the heartbeat");
        assert_eq!(lane_v2(POW_ALGO_ID_PALW_FALLBACK_V1, F - 1, fence()), LaneV2::Other, "an algo-12 header below the fence is nothing");
        assert_eq!(lane_v2(POW_ALGO_ID_PALW_FALLBACK_V1, F, fence()), LaneV2::Fallback);
        assert_eq!(lane_v2(POW_ALGO_ID_PALW_FALLBACK_V1, F, None), LaneV2::Other);
        assert_eq!(lane_v2(POW_ALGO_ID_PALW_FALLBACK_V1, F, Some(ForkActivation::never())), LaneV2::Other, "never() is dormant");
        assert_eq!(lane_v2(1, F, fence()), LaneV2::Other);
    }

    #[test]
    fn below_the_fence_every_lane_defers_to_the_legacy_rule() {
        for lane in [LaneV2::Attempt, LaneV2::Exec, LaneV2::Fallback, LaneV2::LegacyHeartbeat, LaneV2::Retired, LaneV2::Other] {
            for hangs in [false, true] {
                assert_eq!(colour_rule_v2(lane, F - 1, fence(), hangs), ColourRuleV2::Legacy);
                assert_eq!(colour_rule_v2(lane, u64::MAX, None, hangs), ColourRuleV2::Legacy, "no fence, no rule");
                assert_eq!(exec_verdict_v2(lane, F - 1, fence(), hangs), None);
            }
        }
    }

    #[test]
    fn the_colour_table_past_the_fence() {
        use ColourRuleV2::*;
        assert_eq!(colour_rule_v2(LaneV2::Attempt, F, fence(), true), Weighted);
        assert_eq!(colour_rule_v2(LaneV2::Attempt, F, fence(), false), Classic, "F1: an attempt of another branch is coloured against that branch");
        assert_eq!(colour_rule_v2(LaneV2::Fallback, F, fence(), true), Yielding);
        assert_eq!(colour_rule_v2(LaneV2::Fallback, F, fence(), false), Yielding, "a FALLBACK yields whichever branch it hangs from");
        assert_eq!(colour_rule_v2(LaneV2::Exec, F, fence(), true), ExecNonScoring);
        assert_eq!(colour_rule_v2(LaneV2::Exec, F, fence(), false), ExecNonScoring, "the colour never walks; the verdict below splits E-BLUE from RED");
        assert_eq!(exec_verdict_v2(LaneV2::Exec, F, fence(), true), Some(ExecVerdictV2::EBlue));
        assert_eq!(exec_verdict_v2(LaneV2::Exec, F, fence(), false), Some(ExecVerdictV2::Red), "another branch or outside the window: RED, not E-BLUE");
        assert_eq!(exec_verdict_v2(LaneV2::Attempt, F, fence(), true), None, "only an Exec block has an E verdict");
        assert_eq!(colour_rule_v2(LaneV2::Retired, F, fence(), true), Classic, "invalid anyway; the conservative rule counts it");
        assert_eq!(colour_rule_v2(LaneV2::Other, F, fence(), true), Classic);
    }

    /// I2, the rule half: whatever a REAL candidate that hangs from the merging chain meets, a FALLBACK (or legacy heartbeat) peer neither
    /// counts against it nor has its count enlarged; on another branch the same peer counts, classically (no borrowing across forks).
    #[test]
    fn fallback_peers_are_invisible_to_a_real_on_its_own_chain_and_counted_on_another() {
        let weighted = ColourRuleV2::Weighted;
        let classic = ColourRuleV2::Classic;
        for peer in [LaneV2::Fallback, LaneV2::LegacyHeartbeat] {
            assert_eq!(peer_effect_v2(weighted, peer), Some(PeerEffectV2 { counts: false, enlarges: false }), "{peer:?}");
            assert_eq!(peer_effect_v2(classic, peer), Some(PeerEffectV2 { counts: true, enlarges: true }), "{peer:?}: another branch is classic");
        }
        assert_eq!(peer_effect_v2(weighted, LaneV2::Attempt), Some(PeerEffectV2 { counts: true, enlarges: true }), "REAL still counts REAL");
        // A FALLBACK candidate yields to every blue, and only ever enlarges a transparent-lane peer's count: it cannot redden a REAL through a third block.
        assert_eq!(peer_effect_v2(ColourRuleV2::Yielding, LaneV2::Attempt), Some(PeerEffectV2 { counts: true, enlarges: false }));
        assert_eq!(peer_effect_v2(ColourRuleV2::Yielding, LaneV2::Fallback), Some(PeerEffectV2 { counts: true, enlarges: true }));
        // An Exec block is nobody's peer and consumes nobody's budget under any rule; Legacy is not decided here.
        for rule in [weighted, classic, ColourRuleV2::Yielding, ColourRuleV2::ExecNonScoring] {
            assert_eq!(peer_effect_v2(rule, LaneV2::Exec), Some(PeerEffectV2 { counts: false, enlarges: false }), "{rule:?}");
        }
        assert_eq!(peer_effect_v2(ColourRuleV2::ExecNonScoring, LaneV2::Attempt), Some(PeerEffectV2 { counts: false, enlarges: false }));
        assert_eq!(peer_effect_v2(ColourRuleV2::Legacy, LaneV2::Attempt), None);
    }

    /// I2 as a property: the number of peers a REAL candidate counts against itself is independent of how many FALLBACK peers there are.
    #[test]
    fn a_reals_counted_peers_do_not_depend_on_the_fallback_count() {
        let mut rng = Rng(0xFA11_BAC4);
        for _ in 0..500 {
            let reals = rng.next() % 4;
            let fallbacks = rng.next() % 40;
            let counted = |reals: u64, fallbacks: u64| {
                let mut n = 0;
                for _ in 0..reals {
                    n += peer_effect_v2(ColourRuleV2::Weighted, LaneV2::Attempt).unwrap().counts as u64;
                }
                for _ in 0..fallbacks {
                    n += peer_effect_v2(ColourRuleV2::Weighted, LaneV2::Fallback).unwrap().counts as u64;
                }
                n
            };
            assert_eq!(counted(reals, fallbacks), counted(reals, 0), "{fallbacks} fallbacks changed what a REAL counts");
            assert_eq!(counted(reals, fallbacks), reals);
        }
    }

    /// I1: an E-BLUE member adds nothing to blue score, blue work or the k budget, however many there are and however they are flagged.
    #[test]
    fn round_count_never_moves_blue_score_blue_work_or_k() {
        let mut rng = Rng(0xE_B10E);
        let lanes = [LaneV2::Attempt, LaneV2::Fallback, LaneV2::LegacyHeartbeat];
        for _ in 0..500 {
            let n = (rng.next() % 12) as usize;
            let base: Vec<(LaneV2, bool)> = (0..n).map(|_| (lanes[(rng.next() % 3) as usize], rng.next() % 4 != 0)).collect();
            let with_rounds = {
                let mut v = base.clone();
                for _ in 0..(rng.next() % 150) {
                    v.push((LaneV2::Exec, rng.next() % 2 == 0));
                }
                v
            };
            assert_eq!(scoring_delta_v2(&base), scoring_delta_v2(&with_rounds), "rounds moved a scoring quantity");
        }
        // And what the others add: REAL 2^20, FALLBACK ε, a red nothing.
        let d = scoring_delta_v2(&[(LaneV2::Attempt, true), (LaneV2::Fallback, true), (LaneV2::Attempt, false), (LaneV2::Exec, true)]);
        assert_eq!(d, ScoringDeltaV2 { blue_score: 2, blue_work: (1 << 20) + 1, k_members: 2 });
        assert_eq!(scoring_delta_v2(&[]), ScoringDeltaV2::default());
    }

    fn facts(real: u64, nr: Option<u64>, fb: u64, nf: Option<u64>) -> ClockFactsV2 {
        ClockFactsV2 { priced: 0, real, newest_real_ms: nr, fallback: fb, newest_fallback_ms: nf, legacy_beats: 0, newest_legacy_ms: None }
    }

    #[test]
    fn real_carries_first_and_fallback_is_a_reserve_behind_the_grace() {
        let slot = Some(1_000_000);
        // Nothing at the slot: no tick.
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 0, None), slot, G), CarrierV2::None);
        // A REAL stamped at the slot carries immediately.
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(1_000_000), 0, None), slot, G), CarrierV2::Real);
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(999_999), 0, None), slot, G), CarrierV2::None, "a REAL stamped before the slot ticks nothing");
        // A FALLBACK at the slot is not yet a carrier; behind the grace it is.
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 1, Some(1_000_000)), slot, G), CarrierV2::None);
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 1, Some(1_000_000 + G - 1)), slot, G), CarrierV2::None);
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 1, Some(1_000_000 + G)), slot, G), CarrierV2::Fallback);
        // Both qualify: REAL carries, and the tick is +1.
        let both = facts(2, Some(1_000_005), 3, Some(1_000_000 + G + 7));
        assert_eq!(palw_clock_carrier_v2(&both, slot, G), CarrierV2::Real);
        assert!(CarrierV2::Real.granted() && CarrierV2::Fallback.granted() && !CarrierV2::None.granted());
        // A stale REAL (the slow class: stamped with its template's time) leaves the reserve to carry.
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(10), 1, Some(1_000_000 + G)), slot, G), CarrierV2::Fallback);
        // An open slot (no cursor) admits any source; a priced block is the clock and nothing stands in.
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(0), 0, None), None, G), CarrierV2::Real);
        assert_eq!(palw_clock_carrier_v2(&ClockFactsV2 { priced: 1, ..facts(1, Some(u64::MAX), 1, Some(u64::MAX)) }, slot, G), CarrierV2::None);
        // A legacy heartbeat (made below the fence) is a FALLBACK-kind source with no grace.
        let legacy = ClockFactsV2 { legacy_beats: 1, newest_legacy_ms: Some(1_000_000), ..Default::default() };
        assert_eq!(palw_clock_carrier_v2(&legacy, slot, G), CarrierV2::Fallback);
        // The header-stage stamp rule of a FALLBACK.
        assert!(fallback_stamp_admits_v2(1_000_000 + G, slot, G) && !fallback_stamp_admits_v2(1_000_000 + G - 1, slot, G));
        assert!(fallback_stamp_admits_v2(0, None, G), "no cursor, no wait");
        assert!(fallback_stamp_admits_v2(u64::MAX, Some(u64::MAX), G), "saturating, never a panic");
    }

    /// With the grace at zero and the kinds not told apart, the carrier rule IS ADR-0165's tick-source rule followed by the slot test.
    #[test]
    fn with_no_grace_it_is_adr_0165s_rule_byte_for_byte() {
        let mut rng = Rng(0xC0DE_0165);
        for _ in 0..5_000 {
            let slot_ms = (rng.next() % 4 != 0).then(|| rng.next() % 2_000);
            let (nr, nf) = (rng.next() % 5, rng.next() % 5);
            let mut newest = |n: u64| (n > 0).then(|| rng.next() % 2_000);
            let (ms_r, ms_f) = (newest(nr), newest(nf));
            let priced = rng.next() % 3 / 2; // mostly 0, sometimes 1
            let old_facts =
                PalwClockMergesetFactsV1 { priced, heartbeats: nf, attempts: nr, newest_beat_ms: ms_f, newest_attempt_ms: ms_r };
            let (stand_in, beat_ms) = palw_clock_tick_source_v1(&old_facts, true);
            let old_granted = stand_in
                && slot_ms.map(|s| PalwClockCursorV1 { next_slot_ms: s, slots_consumed: 0 }).is_none_or(|c| palw_clock_slot_admits_v1(&c, beat_ms).is_ok());
            let new = ClockFactsV2 { priced, real: nr, newest_real_ms: ms_r, fallback: nf, newest_fallback_ms: ms_f, ..Default::default() };
            assert_eq!(palw_clock_carrier_v2(&new, slot_ms, 0).granted(), old_granted, "{old_facts:?} slot {slot_ms:?}");
        }
    }

    /// **I4 — the clock-safety simulation, with the three-way carrier.** A chain of steps, each merging every source that arrived since the
    /// last one; REAL and FALLBACK stamped by an adversary anywhere from `now` to `now + 132 s`, in bursts of up to 100 at one instant. A
    /// step is stamped at `max(now, slot)` (H5) and refused past `now + 132 s` (the lead cap). Whatever happens: (1) the DAA moves at most ONCE a
    /// step, whatever the mix — REAL + FALLBACK is +1, never +2; (2) ticks are at least one interval apart in stamp; (3) over any horizon the DAA
    /// has advanced at most `horizon / interval + 2` — what a heartbeat miner alone can do.
    #[test]
    fn a_producer_of_any_mix_of_real_and_fallback_cannot_run_the_clock_faster_than_one_a_slot() {
        let mut rng = Rng(0xA11_CA11);
        for grace in [0, G] {
            for _round in 0..300 {
                let start = 10_000_000u64;
                let mut now = start;
                let mut reference = start;
                let mut daa = 0u64;
                let mut last_tick_stamp = start;
                let mut both_in_one_step = 0u32;
                for _ in 0..(40 + rng.next() % 80) {
                    now += rng.next() % (2 * I);
                    let (n_fb, n_real) = (rng.next() % 4, rng.next() % 101);
                    let stamp = |r: u64| now + r % 132_001;
                    let newest = |n: u64, rng: &mut Rng| (n > 0).then(|| (0..n).map(|_| stamp(rng.next())).max().unwrap());
                    let (nf, nr) = (newest(n_fb, &mut rng), newest(n_real, &mut rng));
                    let f = facts(n_real, nr, n_fb, nf);
                    let slot = crate::palw_clock_cursor_v1::palw_clock_cursor_from_reference_v1(reference, I).next_slot_ms;
                    let carrier = palw_clock_carrier_v2(&f, Some(slot), grace);
                    if carrier.granted() {
                        let step_stamp = now.max(slot);
                        if step_stamp > now + 132_000 {
                            continue; // refused by the lead cap: not merged now
                        }
                        if n_real > 0 && n_fb > 0 {
                            both_in_one_step += 1;
                        }
                        daa += 1; // ONE exemption removed, whatever the mix
                        assert!(step_stamp >= last_tick_stamp, "stamps of ticks never go back");
                        assert!(step_stamp - reference >= I, "two ticks are at least one interval apart in stamp");
                        last_tick_stamp = step_stamp;
                        reference = step_stamp;
                    }
                }
                let horizon = (now - start).max(1);
                assert!(daa <= horizon / I + 2, "grace {grace}: {daa} ticks over {horizon} ms (≤ {})", horizon / I + 2);
                let _ = both_in_one_step;
            }
        }
    }

    /// I3: `attempt + Σ round shares ≤ W_claim` for every claim, σ, ticket count and number of rounds credited.
    #[test]
    fn a_claims_weight_is_never_more_than_it_was_whatever_the_rounds() {
        let mut rng = Rng(0xB0_D6E7);
        for _ in 0..3_000 {
            let w = (rng.next() as u128) << (rng.next() % 40);
            let sigma = (rng.next() % 1_001) as u16;
            let n = (rng.next() % 121) as u32;
            let split = claim_weight_split_v2(w, sigma, n).expect("in range");
            assert_eq!(split.attempt + split.pool, w, "the split is a partition of W_claim");
            for rounds in [0, 1.min(n), n / 2, n] {
                assert!(claim_weight_bound_holds_v2(&split, rounds), "W={w} σ={sigma} N={n} rounds={rounds}");
            }
            assert!(!claim_weight_bound_holds_v2(&split, n + 1), "more rounds than tickets is refused");
            // 120 rounds or one: the bound is the same sentence.
            if n == 120 {
                assert!(claim_weight_bound_holds_v2(&split, 120) && claim_weight_bound_holds_v2(&split, 1));
            }
        }
        // σ = 0: weightless rounds; the attempt keeps everything.
        let zero = claim_weight_split_v2(1_000_000, 0, 120).unwrap();
        assert_eq!((zero.attempt, zero.pool, zero.per_round), (1_000_000, 0, 0));
        // σ = 100 permille over 120 tickets.
        let ten = claim_weight_split_v2(1_000_000, 100, 120).unwrap();
        assert_eq!((ten.attempt, ten.pool, ten.per_round), (900_000, 100_000, 833));
        assert!(120 * ten.per_round + ten.attempt <= 1_000_000, "the division's remainder is never created");
        // N = 0: no tickets, no shares. Hostile inputs are refused, never a panic.
        assert_eq!(claim_weight_split_v2(10, 500, 0).unwrap().per_round, 0);
        assert_eq!(claim_weight_split_v2(10, 1_001, 5), None);
        assert_eq!(claim_weight_split_v2(u128::MAX, 1_000, 5), None, "overflow is refused");
        assert_eq!(claim_weight_split_v2(u128::MAX, 0, 5).map(|s| s.attempt), Some(u128::MAX));
    }

    #[test]
    fn the_fence_values_are_the_recommended_defaults_and_hashed_with_it() {
        assert_eq!(palw_accounting_v2_value_v1(), [20_000, 0, 1]);
        assert_eq!(G, crate::palw_real_share_v1::PALW_REAL_TICK_GRACE_MS_V1, "the consensus grace is the grace node policy has used since ADR-0165");
    }
}
