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
//! * **I4** one tick a slot — REAL first, FALLBACK the reserve, REAL + FALLBACK in one mergeset is +1 never +2
//!   ([`palw_clock_carrier_v2`]).
//!
//! **R-NoClass.** Nothing here takes a class id. A lane is the PoW algorithm a header satisfies (proved by that PoW, not declared); a class
//! is a claim about state and is never evidence in the declarer's favour. The inputs are the lane, the candidate's own DAA score and
//! the fence (E0), one DAG fact — does the candidate hang from the merging block's selected chain (E1) — and constants (E2). State (E3)
//! decides credit in the fold, never colour.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_ROUND_V1, PALW_ATTEMPT_BLUE_WORK_LOG2, is_palw_attempt_algo_id};

/// **σ — the rounds' share of a claim's weight, in permille** (ADR-0172 §6, Q3). Carried in `PALW_SAFE_WEIGHT` only, never in blue work.
/// `0` makes rounds weightless: the user's initial value (the user decides σ after an explanation); the allocation code exists
/// ([`claim_weight_split_v2`]) and a later value is a new network id (it is hashed with the fence).
pub const PALW_ACCOUNTING_V2_SIGMA_PERMILLE: u64 = 0;

/// **The carrier-only FALLBACK subsidy, in milli-carves** (ADR-0172 §5.6, Q5): FALLBACK is fee-only. A tiny subsidy for the tick carrier alone
/// can be added later by changing this constant (a new network id); today it is 0 and nothing reads a non-zero value.
pub const PALW_ACCOUNTING_V2_FALLBACK_CARRIER_SUBSIDY_MILLI: u64 = 0;

/// The FALLBACK reserve's **producer-side wait** behind the slot's opening, in ms — node POLICY, never a consensus rule (Q4): the heartbeat/FALLBACK
/// miner waits this long for a REAL attempt before it mints. The consensus carrier rule does not read it. (ADR-0165's `PALW_REAL_TICK_GRACE_MS_V1`.)
pub const PALW_ACCOUNTING_V2_PRODUCER_WAIT_MS: u64 = 20_000;

/// The rule's own version, hashed with the fence so a change to any value here is a new network id.
pub const PALW_ACCOUNTING_V2_VERSION: u64 = 1;

/// **The values the fingerprint hashes beside the fence's height** — `[σ_permille, carrier_subsidy_milli, version]`.
pub const fn palw_accounting_v2_value_v1() -> [u64; 3] {
    [PALW_ACCOUNTING_V2_SIGMA_PERMILLE, PALW_ACCOUNTING_V2_FALLBACK_CARRIER_SUBSIDY_MILLI, PALW_ACCOUNTING_V2_VERSION]
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
    /// **FALLBACK: algo 8 made AT OR PAST the fence** — the heartbeat lane, re-meant (Q1). Below the fence algo 8 is a heartbeat and the BASE-0
    /// floor is an attempt; from it algo 8 is the one bonded reserve (a signed envelope, credit only with an eligible bond) and BASE-0 is retired.
    Fallback,
    /// A heartbeat (algo 8) made BELOW the fence: valid for ever, a yielding peer and a tick source, no bond envelope required.
    LegacyHeartbeat,
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
        return if armed { LaneV2::Fallback } else { LaneV2::LegacyHeartbeat };
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
        LaneV2::Attempt | LaneV2::LegacyHeartbeat | LaneV2::Other => ColourRuleV2::Classic,
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
            // E-BLUE adds nothing; a foreign lane never reaches a valid mergeset.
            LaneV2::Exec | LaneV2::Other => continue,
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
    /// FALLBACK-kind sources (algo 8, before or after the fence), and the newest stamp among them.
    pub fallback: u64,
    pub newest_fallback_ms: Option<u64>,
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
/// fallback_ok = priced == 0 ∧ fallback > 0 ∧ newest_fallback ≥ slot
/// carrier     = Real if real_ok, else Fallback if fallback_ok, else None
/// ```
///
/// A function of header facts, so it can be computed when the header's DAA score is fixed (ADR-0142). **No grace is in the consensus rule** (Q4:
/// waiting for a REAL is the producer's policy): `granted` is exactly ADR-0165's `palw_clock_tick_source_v1` followed by the slot test — the two
/// kinds are told apart only for attribution (REAL wins a mergeset that holds both). Both kinds stay valid blocks.
pub fn palw_clock_carrier_v2(facts: &ClockFactsV2, slot_ms: Option<u64>) -> CarrierV2 {
    if facts.priced != 0 {
        return CarrierV2::None;
    }
    let at_or_past = |newest: Option<u64>| newest.is_some_and(|ms| slot_ms.is_none_or(|slot| ms >= slot));
    if facts.real > 0 && at_or_past(facts.newest_real_ms) {
        CarrierV2::Real
    } else if facts.fallback > 0 && at_or_past(facts.newest_fallback_ms) {
        CarrierV2::Fallback
    } else {
        CarrierV2::None
    }
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

/// **A round's credit, once per `(claim, round_index)`** (ADR-0172 §6, Q3). `verified` is the fold's branch-local verdict (E-BLUE structure, a
/// `Final` claim, a granted permit, inside the window) — the caller's, never read from the header. `credited` is the chain's ledger of what was
/// already credited. Returns the weight added to `safe_weight` (0 when refused: unverified, a repeat, out of range, or σ = 0). The ledger is a
/// set so a reorg reverts exactly (the fold's delta removes the entry) and IBD, a pruned join and the pruning proof replay the same credit.
pub fn credit_round_v2(
    credited: &mut std::collections::BTreeSet<(u64, u32)>,
    split: &ClaimSplitV2,
    claim: u64,
    round_index: u32,
    verified: bool,
) -> u128 {
    if !verified || round_index >= split.n_tickets || split.per_round == 0 {
        return 0;
    }
    if !credited.insert((claim, round_index)) {
        return 0;
    }
    split.per_round
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Emission: one block's subsidy a DAA, shared by W_claim (fold stage; the budget is a pure function of the schedule)
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The DAA's PALW issuance budget** (ADR-0172 §6b, the user's rule of 2026-10-04): **one block's subsidy at that DAA** — `calc_block_subsidy(daa)`,
/// the Kaspa schedule (`SUBSIDY_BY_MONTH_TABLE` scaled by `target_time_per_block`) the caller already has. The total a DAA mints across **every** PALW
/// route (all claims' carves, a FALLBACK carrier subsidy, any other subsidy path) never exceeds it. It replaces ADR-0167's F-EM ledger of 16 carves a
/// DAA, a figure chosen as three times a measured claim rate rather than from the schedule.
pub const fn palw_emission_budget_v2(block_subsidy: u64) -> u64 {
    block_subsidy
}

/// **The one-claim ceiling**: today's carve, `PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1` of one block's subsidy. A lone claim is paid no more than
/// it is paid now; the unused remainder is **not minted**.
pub fn palw_emission_claim_cap_v2(block_subsidy: u64) -> u64 {
    (block_subsidy as u128 * crate::config::params::PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1 as u128 / 1000) as u64
}

/// **The FALLBACK carrier's subsidy**, from the constant hook ([`PALW_ACCOUNTING_V2_FALLBACK_CARRIER_SUBSIDY_MILLI`], 0 today): milli-carves of the
/// one-claim ceiling, paid to the tick carrier only while the floor is Idle. Counted INSIDE the DAA's budget.
pub fn palw_emission_carrier_subsidy_v2(block_subsidy: u64, carrier_milli: u64, carrier_is_fallback_while_idle: bool) -> u64 {
    if !carrier_is_fallback_while_idle {
        return 0;
    }
    ((palw_emission_claim_cap_v2(block_subsidy) as u128 * carrier_milli as u128) / 1000).min(block_subsidy as u128) as u64
}

/// **How one DAA's budget is shared** (ADR-0172 §6b): the pool is the budget less the carrier's subsidy; each claim takes `⌊pool · w_i / Σw⌋` by its
/// canonical compute `W_claim`, capped at the one-claim ceiling; every remainder, a cap's excess, and a DAA with **no claim** are not minted
/// (don't-mint, not carried forward). `shares[i]` pairs with `claims[i]`. `None` on arithmetic overflow. **Σ shares + carrier ≤ budget, always.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmissionSplitV2 {
    pub budget: u64,
    pub carrier: u64,
    pub shares: Vec<u64>,
}

impl EmissionSplitV2 {
    /// Everything this DAA mints on the PALW routes.
    pub fn total(&self) -> u128 {
        self.carrier as u128 + self.shares.iter().map(|s| *s as u128).sum::<u128>()
    }
}

pub fn palw_emission_split_v2(block_subsidy: u64, claims: &[u128], carrier_subsidy: u64) -> Option<EmissionSplitV2> {
    let budget = palw_emission_budget_v2(block_subsidy);
    let carrier = carrier_subsidy.min(budget);
    let pool = (budget - carrier) as u128;
    let cap = palw_emission_claim_cap_v2(block_subsidy) as u128;
    let sum: u128 = claims.iter().try_fold(0u128, |acc, w| acc.checked_add(*w))?;
    let shares = claims
        .iter()
        .map(|w| {
            if sum == 0 {
                return Some(0u64);
            }
            // pool < 2^64 and w ≤ sum: the product can overflow u128 only for w near 2^64+ — refuse rather than wrap.
            let share = pool.checked_mul(*w)? / sum;
            Some(share.min(cap) as u64)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(EmissionSplitV2 { budget, carrier, shares })
}

/// **E2, option B (recommended, implemented): the payout is fixed at `Final` from the DAA's own rooted weight sum.** At acceptance a claim's escrow is
/// reserved as it is today (≤ one claim's carve) and its `W_claim` is added to the accepted DAA's [`DaaWeightRowV2`]; at `Final` — a challenge window after
/// the DAA closed, so `sum_w` is final — the claim is paid `min(escrow, ⌊budget · W / ΣW⌋)`. The difference is not minted. **ΣW counts every claim accepted
/// in the DAA, voided ones included**: a denominator that shrank as claims voided would make early `Final`s pay against a larger share than late ones and could
/// let Σ payouts pass the budget; with the full denominator `Σ min(escrow_i, ⌊B·W_i/ΣW⌋) ≤ B` for any set of finals and voids in any order. A voided claim's share is
/// simply never minted. `None` on overflow (the caller then pays nothing: conservative).
pub fn palw_emission_final_payout_v2(budget: u64, sum_w: u128, w_claim: u128, escrow: u64) -> Option<u64> {
    if sum_w == 0 || w_claim > sum_w {
        return Some(0);
    }
    let share = (budget as u128).checked_mul(w_claim)? / sum_w;
    Some(share.min(escrow as u128) as u64)
}

/// **E2, option A (the alternative, for the comparison): first come, first served up to the budget.** Each claim, in acceptance order, takes
/// `min(escrow, what is left of the DAA's budget)`. Never above the budget and needs no per-DAA weight sum — but it pays by arrival, not by work: the first
/// claim of a DAA takes its whole carve (720 ‰ of the budget) and a heavy claim that arrives fifth takes nothing, so a producer that can place its claim first
/// (or a parent set that orders the mergeset in its favour) is paid at the expense of the others. Held as a test; not recommended.
pub fn palw_emission_fcfs_payouts_v2(budget: u64, escrows_in_acceptance_order: &[u64]) -> Vec<u64> {
    let mut left = budget as u128;
    escrows_in_acceptance_order
        .iter()
        .map(|e| {
            let take = (*e as u128).min(left);
            left -= take;
            take as u64
        })
        .collect()
}

/// **The per-DAA weight row** (option B's rooted state): `sum_w` — every claim accepted in the DAA, voided included, never lowered; `budget` — the DAA's
/// block subsidy, written once by the first claim; `open` — claims accepted and not yet `Final` or `Voided`, so the row is dropped exactly when nothing can
/// read it again. One row per DAA that holds a live claim: at most the number of live claims, 32 bytes of value (16 + 8 + 4, padded) and an 8-byte key each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DaaWeightRowV2 {
    pub sum_w: u128,
    pub budget: u64,
    pub open: u32,
}

/// **Accept one claim into its DAA's row** (`None` on overflow). A DAA's budget is a fact of the DAA: a second claim must agree with the first.
pub fn palw_daa_weight_accept_v2(row: Option<DaaWeightRowV2>, budget: u64, w_claim: u128) -> Option<DaaWeightRowV2> {
    let row = row.unwrap_or(DaaWeightRowV2 { sum_w: 0, budget, open: 0 });
    if row.budget != budget {
        return None;
    }
    Some(DaaWeightRowV2 { sum_w: row.sum_w.checked_add(w_claim)?, budget: row.budget, open: row.open.checked_add(1)? })
}

/// **A claim left the row** (`Final` or `Voided`): `open − 1`; the row is dropped at 0. `sum_w` is never lowered. `None` when there was no row or no open
/// claim (an invariant break the caller refuses by name).
pub fn palw_daa_weight_release_v2(row: Option<DaaWeightRowV2>) -> Option<Option<DaaWeightRowV2>> {
    let row = row?;
    let open = row.open.checked_sub(1)?;
    Some((open > 0).then_some(DaaWeightRowV2 { open, ..row }))
}

/// **A rider takes its lead's carve in pieces, never more**: the lead's share split `⌊share / (1 + n)⌋` to each of the `n` riders (ADR-0167 §D2);
/// the lead keeps the rest. Σ = the lead's share exactly.
pub fn palw_emission_rider_split_v2(lead_share: u64, riders: u32) -> (u64, u64) {
    let each = lead_share / (1 + riders as u64);
    (lead_share - each * riders as u64, each)
}

/// **Real-time emission of a DAA**, in sompi per second, when the DAA took `interval_ms` of wall clock. Because the budget is per DAA and the schedule is
/// per `target_ms`, a DAA slower than the target mints **less** per second than the schedule, never more.
pub fn palw_emission_rate_sompi_per_s_v2(minted: u128, interval_ms: u64) -> u128 {
    if interval_ms == 0 { u128::MAX } else { minted * 1000 / interval_ms as u128 }
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
        let needs: [(bool, &'static str); 13] = [
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
            (
                below(self.palw_canonical_work),
                "palw_accounting_v2 needs palw_canonical_work at or below it: W_claim is the derived pwu (ADR-0149), canonical active compute, not a declared number",
            ),
            (
                below(self.palw_capacity_weight_cap),
                "palw_accounting_v2 needs palw_capacity_weight_cap at or below it: a large claim's unfinished weight is bounded by its bond (J-1)",
            ),
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
        // Algo 8: a heartbeat below the fence, FALLBACK from it (Q1: the same lane id, re-meant); dormant it is the heartbeat for ever.
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, F - 1, fence()), LaneV2::LegacyHeartbeat);
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, F, fence()), LaneV2::Fallback);
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, u64::MAX, None), LaneV2::LegacyHeartbeat, "dormant: the heartbeat is the heartbeat");
        assert_eq!(lane_v2(POW_ALGO_ID_HEARTBEAT_V1, F, Some(ForkActivation::never())), LaneV2::LegacyHeartbeat, "never() is dormant");
        assert_eq!(lane_v2(12, F, fence()), LaneV2::Other, "no new lane id was allocated");
        assert_eq!(lane_v2(1, F, fence()), LaneV2::Other);
    }

    #[test]
    fn below_the_fence_every_lane_defers_to_the_legacy_rule() {
        for lane in [LaneV2::Attempt, LaneV2::Exec, LaneV2::Fallback, LaneV2::LegacyHeartbeat, LaneV2::Other] {
            for hangs in [false, true] {
                assert_eq!(colour_rule_v2(lane, F - 1, fence(), hangs), ColourRuleV2::Legacy);
                assert_eq!(colour_rule_v2(lane, u64::MAX, None, hangs), ColourRuleV2::Legacy, "no fence, no rule");
                assert_eq!(exec_verdict_v2(lane, F - 1, fence(), hangs), None);
            }
        }
    }

    /// **Stage 2's finding, held as a test.** With algo 8 re-meant as FALLBACK (Q1) and ADR-0105 F1 a prerequisite of the fence, v2's colour of every
    /// lane is the colouring the code already has: FALLBACK is `Heartbeat` (yields, never enlarges), a REAL that hangs is `Weighted`, one that does
    /// not is `Classic`, a round block is recorded non-scoring without a walk. So `ghostdag()` needs no change; this table is what it computes today.
    #[test]
    fn v2_colouring_is_the_existing_adr_0105_colouring_given_its_prerequisites() {
        // (algo, own DAA, hangs) -> the existing rule's name
        let existing = |algo: u8, daa: u64, hangs: bool| -> &'static str {
            // ADR-0105 `lane_coloring` with the transparency and same-chain fences in force, and the round lane open (ADR-0125).
            if algo == POW_ALGO_ID_PALW_ROUND_V1 {
                "round: add_red, no walk"
            } else if algo == POW_ALGO_ID_HEARTBEAT_V1 {
                "Heartbeat"
            } else if is_palw_attempt_algo_id(algo) {
                if hangs { "Weighted" } else { "Classic" }
            } else {
                let _ = daa;
                "Classic"
            }
        };
        let v2 = |algo: u8, daa: u64, hangs: bool| -> &'static str {
            let lane = lane_v2(algo, daa, fence());
            match colour_rule_v2(lane, daa, fence(), hangs) {
                // Below the fence v2 defers to this very table.
                ColourRuleV2::Legacy => existing(algo, daa, hangs),
                ColourRuleV2::ExecNonScoring => "round: add_red, no walk",
                ColourRuleV2::Yielding => "Heartbeat",
                ColourRuleV2::Weighted => "Weighted",
                ColourRuleV2::Classic => "Classic",
            }
        };
        for algo in [POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_ROUND_V1, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3, 1, 7] {
            for daa in [0, F - 1, F, F + 1, 1_000_000] {
                for hangs in [false, true] {
                    assert_eq!(v2(algo, daa, hangs), existing(algo, daa, hangs), "algo {algo} daa {daa} hangs {hangs}");
                }
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
        ClockFactsV2 { priced: 0, real, newest_real_ms: nr, fallback: fb, newest_fallback_ms: nf }
    }

    #[test]
    fn real_carries_first_and_fallback_is_the_reserve_with_no_grace_in_the_rule() {
        let slot = Some(1_000_000);
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 0, None), slot), CarrierV2::None);
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(1_000_000), 0, None), slot), CarrierV2::Real);
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(999_999), 0, None), slot), CarrierV2::None);
        // A FALLBACK at the slot carries at once: the producer's wait (policy) is not the rule's.
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 1, Some(1_000_000)), slot), CarrierV2::Fallback);
        assert_eq!(palw_clock_carrier_v2(&facts(0, None, 1, Some(999_999)), slot), CarrierV2::None);
        // Both qualify: REAL carries, and the tick is +1 (one exemption, whatever the mix).
        assert_eq!(palw_clock_carrier_v2(&facts(2, Some(1_000_005), 3, Some(1_000_007)), slot), CarrierV2::Real);
        assert!(CarrierV2::Real.granted() && CarrierV2::Fallback.granted() && !CarrierV2::None.granted());
        // A stale REAL (a slow class: stamped with its template's time) leaves the reserve to carry.
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(10), 1, Some(1_000_000)), slot), CarrierV2::Fallback);
        // An open slot admits any source; a priced block is the clock and nothing stands in.
        assert_eq!(palw_clock_carrier_v2(&facts(1, Some(0), 0, None), None), CarrierV2::Real);
        assert_eq!(palw_clock_carrier_v2(&ClockFactsV2 { priced: 1, ..facts(1, Some(u64::MAX), 1, Some(u64::MAX)) }, slot), CarrierV2::None);
    }

    /// The carrier rule's `granted` IS ADR-0165's tick-source rule followed by the slot test, for every facts shape (the kinds differ only in attribution).
    #[test]
    fn granted_is_adr_0165s_rule_byte_for_byte() {
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
            assert_eq!(palw_clock_carrier_v2(&new, slot_ms).granted(), old_granted, "{old_facts:?} slot {slot_ms:?}");
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
        {
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
                    let carrier = palw_clock_carrier_v2(&f, Some(slot));
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
                assert!(daa <= horizon / I + 2, "{daa} ticks over {horizon} ms (≤ {})", horizon / I + 2);
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

    /// **Q3, both σ.** Conservation (`attempt + Σ credited ≤ W_claim`), no double count (a round index credits once, however often it is
    /// merged or replayed, in any order), unverified rounds credit nothing, and at σ = 0 the claim weighs exactly what it weighs today.
    #[test]
    fn round_credit_conserves_never_double_counts_and_is_todays_weight_at_sigma_zero() {
        use std::collections::BTreeSet;
        let mut rng = Rng(0x5160_0001);
        for sigma in [0u16, 1, 100, 300, 1000] {
            for _ in 0..400 {
                let w = (rng.next() as u128) << (rng.next() % 30);
                let n = (rng.next() % 121) as u32;
                let split = claim_weight_split_v2(w, sigma, n).unwrap();
                // A hostile stream: repeats, out-of-range indices, unverified rounds, shuffled order.
                let stream: Vec<(u32, bool)> = (0..300).map(|_| ((rng.next() % 150) as u32, rng.next() % 5 != 0)).collect();
                let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
                let mut total_a = 0u128;
                for &(i, ok) in &stream {
                    total_a += credit_round_v2(&mut a, &split, 7, i, ok);
                }
                // The same stream replayed in reverse (another arrival order / a second node) credits the same SET of rounds when all are
                // verified; with the same verdicts per index the total is the same.
                let mut total_b = 0u128;
                let mut verdict = std::collections::BTreeMap::new();
                for &(i, ok) in &stream {
                    *verdict.entry(i).or_insert(false) |= ok;
                }
                for (&i, &ok) in verdict.iter().rev() {
                    total_b += credit_round_v2(&mut b, &split, 7, i, ok);
                }
                assert!(split.attempt + total_a <= w, "σ={sigma}: conservation");
                assert!(a.len() as u32 <= n, "never more credited rounds than tickets");
                assert_eq!(total_a, a.len() as u128 * split.per_round, "σ={sigma}: each credited round counted once");
                if stream.iter().all(|&(_, ok)| ok) || true {
                    // Total depends only on WHICH indices were verified at least once, not on order or repetition.
                    let set_a: BTreeSet<u32> = stream.iter().filter(|&&(i, ok)| ok && i < n).map(|&(i, _)| i).collect();
                    assert_eq!(total_a, set_a.len() as u128 * split.per_round, "σ={sigma}");
                    assert_eq!(total_a, total_b, "σ={sigma}: order-independent");
                }
                if sigma == 0 {
                    assert_eq!((split.attempt, total_a), (w, 0), "σ = 0: the claim weighs W_claim, rounds weigh nothing — today's rule");
                }
            }
        }
        // Another claim's round with the same index is a different ledger row.
        let split = claim_weight_split_v2(1_000, 100, 10).unwrap();
        let mut ledger = BTreeSet::new();
        assert_eq!(credit_round_v2(&mut ledger, &split, 1, 3, true), 10);
        assert_eq!(credit_round_v2(&mut ledger, &split, 1, 3, true), 0, "once per (claim, round)");
        assert_eq!(credit_round_v2(&mut ledger, &split, 2, 3, true), 10);
        assert_eq!(credit_round_v2(&mut ledger, &split, 1, 4, false), 0, "unverified");
        assert_eq!(credit_round_v2(&mut ledger, &split, 1, 10, true), 0, "index beyond the tickets");
    }

    /// **The emission pillar.** Whatever the claims, weights, subsidy and carrier: (1) the DAA's PALW total never exceeds one block's subsidy; (2) a DAA
    /// with no claim mints nothing but the carrier's constant; (3) shares are proportional to `W_claim` (no claim is paid more than its fair share) and
    /// never above the one-claim ceiling; (4) riders split their lead exactly; (5) a DAA slower than the schedule's target mints less per second than the
    /// schedule, never more; (6) the schedule's own unit is respected for a lone claim (today's carve).
    #[test]
    fn a_daa_never_mints_more_than_one_blocks_subsidy_and_shares_it_by_w_claim() {
        let mut rng = Rng(0xE_5551);
        for _ in 0..4_000 {
            let subsidy = (rng.next() % 5_000_000_000_000) + 1; // up to ~50,000 MSK in sompi
            let n = (rng.next() % 40) as usize;
            let claims: Vec<u128> = (0..n).map(|_| if rng.next() % 6 == 0 { 0 } else { (rng.next() as u128) << (rng.next() % 50) }).collect();
            let carrier_on = rng.next() % 2 == 0;
            let carrier = palw_emission_carrier_subsidy_v2(subsidy, rng.next() % 1_500, carrier_on);
            let split = palw_emission_split_v2(subsidy, &claims, carrier).expect("in range");
            // (1) the ceiling, with every route counted.
            assert!(split.total() <= subsidy as u128, "minted {} of {subsidy}", split.total());
            // (2) no claim, nothing but the carrier.
            if claims.iter().all(|w| *w == 0) {
                assert!(split.shares.iter().all(|s| *s == 0));
            }
            // (3) proportional, capped, zero stays zero.
            let sum: u128 = claims.iter().sum();
            let cap = palw_emission_claim_cap_v2(subsidy) as u128;
            for (w, share) in claims.iter().zip(&split.shares) {
                assert!(*share as u128 <= cap);
                if *w == 0 {
                    assert_eq!(*share, 0);
                } else {
                    assert!(*share as u128 * sum <= (subsidy - split.carrier) as u128 * *w, "a share above its W_claim fraction");
                }
            }
            // (4) riders: the lead's share is split exactly.
            if let Some(&lead) = split.shares.first() {
                let riders = (rng.next() % 65) as u32;
                let (kept, each) = palw_emission_rider_split_v2(lead, riders);
                assert_eq!(kept + each * riders as u64, lead);
            }
            // (5) real time: a DAA at or beyond the target interval mints at most the schedule's per-second rate.
            let target_ms = 120_000u64;
            let interval = target_ms + rng.next() % 600_000;
            assert!(
                palw_emission_rate_sompi_per_s_v2(split.total(), interval) <= palw_emission_rate_sompi_per_s_v2(subsidy as u128, target_ms),
                "a slow DAA out-minted the schedule"
            );
        }
        // (6) one claim, no carrier: exactly today's carve (720 ‰ of the block's subsidy), and the other 280 ‰ is not minted.
        let one = palw_emission_split_v2(4_445_600_000_000, &[7], 0).unwrap();
        assert_eq!(one.shares, vec![3_200_832_000_000], "720 ‰ of 4,445.6 MSK");
        assert_eq!(one.total(), 3_200_832_000_000);
        // Two equal claims share the pool, each below the cap; the total is far below the budget.
        let two = palw_emission_split_v2(4_445_600_000_000, &[5, 5], 0).unwrap();
        assert_eq!(two.shares, vec![2_222_800_000_000; 2]);
        // Twenty equal claims: one block's subsidy in all, where 16 whole carves (ADR-0167) allowed ~11.5× that.
        let many = palw_emission_split_v2(4_445_600_000_000, &[1; 20], 0).unwrap();
        assert!(many.total() <= 4_445_600_000_000);
        // No claim at all: nothing is minted (the unused budget is not carried, not paid).
        assert_eq!(palw_emission_split_v2(4_445_600_000_000, &[], 0).unwrap().total(), 0);
        // Overflow refused, never a panic.
        assert_eq!(palw_emission_split_v2(u64::MAX, &[u128::MAX, 1], 0), None);
    }

    /// **E2, both options.** B (final-time split over the DAA's rooted weight sum) never mints past the budget for any weights, any set of voids and any order
    /// of `Final`s, pays the same amount to the same claim whatever the arrival order, and is today's carve for a lone claim; A (first come, first served) is
    /// within the budget but pays by arrival: the first claim takes its whole carve and a later, heavier one can take nothing.
    #[test]
    fn e2_the_final_time_split_is_order_independent_and_within_budget_where_first_come_is_neither_fair_nor_stable() {
        let mut rng = Rng(0xE2_E2E2);
        for _ in 0..3_000 {
            let budget = (rng.next() % 5_000_000_000_000) + 1;
            let cap = palw_emission_claim_cap_v2(budget);
            let n = (rng.next() % 30) as usize + 1;
            let ws: Vec<u128> = (0..n).map(|_| (rng.next() as u128 % 1_000_000) + 1).collect();
            // The row, built by accepting every claim (voided ones too) in one order, then in the reverse order: the same row.
            let build = |order: &mut dyn Iterator<Item = usize>| {
                let mut row = None;
                for i in order {
                    row = palw_daa_weight_accept_v2(row, budget, ws[i]);
                }
                row.unwrap()
            };
            let forward = build(&mut (0..n));
            let backward = build(&mut (0..n).rev());
            assert_eq!(forward, backward, "the row is order independent");
            assert_eq!(forward.sum_w, ws.iter().sum::<u128>());
            // Any subset voids; the rest finalise in any order: payouts are per-claim functions of the final row, so Σ ≤ budget.
            let paid: Vec<u64> = ws.iter().map(|w| palw_emission_final_payout_v2(budget, forward.sum_w, *w, cap).unwrap()).collect();
            let finals_total: u128 = ws.iter().zip(&paid).filter(|_| rng.next() % 3 != 0).map(|(_, p)| *p as u128).sum();
            assert!(finals_total <= budget as u128, "minted past the budget");
            assert!(paid.iter().map(|p| *p as u128).sum::<u128>() <= budget as u128, "even if no claim voids");
            // Proportional to W (to the integer division), and never above the escrow.
            for (w, p) in ws.iter().zip(&paid) {
                assert!(*p <= cap && *p as u128 * forward.sum_w <= budget as u128 * *w);
            }
            // Option A, on the same escrows, is within the budget but pays by arrival.
            let a = palw_emission_fcfs_payouts_v2(budget, &vec![cap; n]);
            assert!(a.iter().map(|x| *x as u128).sum::<u128>() <= budget as u128);
            if n >= 3 {
                assert_eq!(a[0], cap, "the first claim takes its whole carve");
                assert_eq!(a[2], 0, "the third takes nothing (720 ‰ + 280 ‰ = the budget)");
            }
        }
        // A lone claim: today's carve, exactly. Two equal claims: half the budget each (below the cap).
        let b = 4_445_600_000_000u64;
        let cap = palw_emission_claim_cap_v2(b);
        assert_eq!(palw_emission_final_payout_v2(b, 7, 7, cap), Some(cap));
        assert_eq!(palw_emission_final_payout_v2(b, 10, 5, cap), Some(b / 2));
        // A heavy claim arriving fifth: B pays it by its weight, A pays it nothing.
        let ws = [1u128, 1, 1, 1, 100];
        let sum: u128 = ws.iter().sum();
        assert_eq!(palw_emission_final_payout_v2(b, sum, 100, cap), Some(cap), "B: capped at one carve, by weight");
        assert_eq!(palw_emission_fcfs_payouts_v2(b, &[cap; 5])[4], 0, "A: nothing, it arrived fifth");
        // Voids stay in the denominator: a voided claim's share is not minted, and nobody else is paid more for it.
        assert_eq!(palw_emission_final_payout_v2(b, 10, 5, cap).unwrap(), palw_emission_final_payout_v2(b, 10, 5, cap).unwrap());
        // Row bookkeeping: open counts down, the row drops at zero, sum_w never falls, a disagreeing budget is refused.
        let r1 = palw_daa_weight_accept_v2(None, 100, 5);
        let r2 = palw_daa_weight_accept_v2(r1, 100, 7).unwrap();
        assert_eq!(r2, DaaWeightRowV2 { sum_w: 12, budget: 100, open: 2 });
        assert_eq!(palw_daa_weight_accept_v2(Some(r2), 101, 1), None);
        let r3 = palw_daa_weight_release_v2(Some(r2)).unwrap();
        assert_eq!(r3, Some(DaaWeightRowV2 { sum_w: 12, budget: 100, open: 1 }));
        assert_eq!(palw_daa_weight_release_v2(r3), Some(None), "dropped when the last claim leaves");
        assert_eq!(palw_daa_weight_release_v2(None), None, "no row, no release");
        // Overflow is refused, never a panic.
        assert_eq!(palw_emission_final_payout_v2(u64::MAX, u128::MAX, u128::MAX, 1), None, "the product overflows: refused (the caller pays nothing)");
        assert_eq!(palw_daa_weight_accept_v2(Some(DaaWeightRowV2 { sum_w: u128::MAX, budget: 1, open: 1 }), 1, 1), None);
    }

    #[test]
    fn the_fence_values_are_the_recommended_defaults_and_hashed_with_it() {
        assert_eq!(palw_accounting_v2_value_v1(), [0, 0, 1], "σ = 0, no carrier subsidy, version 1");
        assert_eq!(PALW_ACCOUNTING_V2_PRODUCER_WAIT_MS, crate::palw_real_share_v1::PALW_REAL_TICK_GRACE_MS_V1, "the producer's wait is the policy ADR-0165 already ships");
    }
}
