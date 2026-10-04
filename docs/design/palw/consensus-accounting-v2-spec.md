# Consensus accounting v2 — implementation spec (ADR-0172)

Status: DESIGN + DORMANT SKELETON, 2026-10-04, branch `rcore/consensus-accounting-v2` (lane AC). The fence `palw_accounting_v2` is `None` on every
preset. Read [ADR-0172](../../adr/0172-three-block-tiers-and-one-tick-a-slot-c-blue-carries-the-chain-e-blue-carries-the-claim-fallback-is-the-one-transparent-reserve.md)
first; this file is the how.

## 1. Fence, constants, allocations

| item | value |
|---|---|
| fence | `Params::palw_accounting_v2: Option<ForkActivation>` (bare height; companion values hashed with it) |
| companion values (`palw_accounting_v2_value_v1()`) | `[G_ms = 20_000, σ_permille = 0, version = 1]` (a `w_fb` value joins when Q11 is decided) |
| FALLBACK lane | `POW_ALGO_ID_PALW_FALLBACK_V1 = 12` (Q1) |
| state | delta 130 `FallbackWeight`, 131 `FallbackBondSlot`, 132 `ExecCredit`, 133 `AccountingMeta`; carriage tail `0xF1`; root block `accounting_v2/v1` Some-only |
| prerequisites (at or below F) | `palw_consensus_mode = ConsensusV2`, `palw_floor_reserve_v1`, `palw_real_clock_tick_v1`, `palw_clock_cursor`, `palw_clock_floor`, `palw_clock_lead_cap`, `palw_anchor_clock`, `palw_single_lottery`, `palw_heartbeat_transparent`, `palw_heartbeat_transparent_same_chain` (F1), `palw_execution_lane`, `palw_anchor_window_v1`, `palw_model_registry`, the capacity budget (`W_claim`) |
| mutually exclusive (must be `None`) | `palw_exec_class_v1`, `palw_ws_clock_v1`, `palw_merge_admission_v1`, `palw_work_slice_v1` |
| written in | the four places: the field; `for_each_fence` (Some-only, at the tail); Some-only `consensus_params_id` + `consensus_schedule_id` with the value array; the `never() -> None` collapse; plus `palw_fences_v1`, the fork-id probe arm (`fork_id_v1.rs`), `override_params`, `validate_palw_v2` (last) |

## 2. Pure functions (consensus-core, `palw_accounting_v2.rs`)

```rust
pub enum LaneV2 { Attempt, Exec, Fallback, LegacyHeartbeat, Other }

/// Lane from the header's algo id, its own DAA score and the fence. E0 only: never reads a class.
pub fn lane_v2(algo_id: u8, daa_score: u64, fence: Option<ForkActivation>) -> LaneV2;

pub enum ColourRuleV2 {
    Legacy,            // below the fence: ADR-0105's rule, byte for byte
    Classic,           // k-cluster over every blue (F1: attempt that does not hang from the merging chain)
    Weighted,          // transparent to FALLBACK / legacy-heartbeat peers; merge-depth floor after a skip
    Yielding,          // FALLBACK: counted against every blue, never enlarges a C block's count
    ExecNonScoring,    // recorded non-scoring, never walked, never a peer
}
pub fn colour_rule_v2(lane: LaneV2, candidate_daa: u64, fence: Option<ForkActivation>, hangs_from_merging_chain: bool) -> ColourRuleV2;

pub enum ExecVerdictV2 { EBlue, Red }
pub fn exec_verdict_v2(lane: LaneV2, candidate_daa: u64, fence: Option<ForkActivation>, hangs_from_merging_chain: bool) -> ExecVerdictV2;

/// Does a peer count against a candidate coloured `rule`? (the one direction the lanes stop counting)
pub fn peer_counts_v2(rule: ColourRuleV2, peer: LaneV2) -> bool;

/// What a block adds to the scoring quantities (I1). `exec` members add nothing.
pub fn scoring_delta_v2(members: &[(LaneV2, bool /*blue*/)]) -> ScoringDeltaV2 { blue_score, blue_work, k_peers }

pub struct ClockFactsV2 { priced: u64, real: u64, newest_real_ms: Option<u64>, fallback: u64, newest_fallback_ms: Option<u64>, legacy_beats: u64, newest_legacy_ms: Option<u64> }
pub enum CarrierV2 { None, Real, Fallback }
pub fn palw_clock_carrier_v2(facts: &ClockFactsV2, slot_ms: Option<u64>, grace_ms: u64) -> CarrierV2;   // granted = carrier != None
pub fn fallback_stamp_admits_v2(stamp_ms: u64, slot_ms: Option<u64>, grace_ms: u64) -> bool;          // header stage, H3 analogue

pub fn claim_weight_split_v2(w_claim: u128, sigma_permille: u16, n_tickets: u32) -> Option<ClaimSplitV2>; // { attempt, pool, per_round }
pub fn claim_weight_bound_holds_v2(split: &ClaimSplitV2, rounds_credited: u32) -> bool;
```

All arithmetic checked or saturating (release has overflow checks; a panic in block processing halts the network). All hostile inputs are refused
by name or answered conservatively, never a panic.

## 3. Colouring (GHOSTDAG) — what changes, site by site

`consensus/src/processes/ghostdag/protocol.rs` (the *only* GHOSTDAG file):

* `lane_coloring()` gains a v2 arm keyed on the candidate's own DAA; below the fence it returns the current value untouched (dormancy, I6).
* `is_heartbeat_block()` generalises to `is_transparent_lane_block()`: algo 12 and legacy algo 8. A `Weighted` candidate skips such peers exactly
  as it skips heartbeats today (including the `skipped_a_heartbeat` flag and `weighted_floor`).
* **Exec candidates** keep the existing `round_flags` path (`add_red`, no walk, not counted in `uncolored_non_round`). No GHOSTDAG output
  changes for them: E-BLUE is a *derived verdict* over `mergeset_reds` (`exec_verdict_v2`), not a stored field.
* `palw_lane_blue_work_v1`: algo 12 → ε (`HEARTBEAT_BLUE_WORK_EPSILON`); algo 10 stays 0 (`algo_id_carries_no_chain_position`).
* Four sites carry the same constructor parameter (header path `services.rs`, the virtual, pruning-proof build and validate through
  `GhostdagManager::new`/`with_level`), as `heartbeat_transparent` does today.
* `algo_id_derives_no_block_level` and `algo_id_is_priced_by_bits*` list algo 12 (Q14); an audit of every predicate that lists algo 8 is a stage-1
  deliverable (ADR-0105's lesson).

## 4. Clock (difficulty.rs, `palw_clock_step_v1`)

Past F the tick-source computation calls `palw_clock_carrier_v2` over `ClockFactsV2` built from the same header loop (one more `else if` for
algo 12; legacy algo 8 counted as `legacy_beats`, G = 0). `granted = carrier != None`; the cursor, reference (earliest tied step), H3 (extended to
algo 12 through `fallback_stamp_admits_v2`), H5 and the lead cap (`lead_capped` lists algo 12) are unchanged. `PalwClockStepV1` gains `carrier`
(telemetry and the accounting record; no rule reads it). Below F the existing `palw_clock_tick_source_v1` is called unchanged.

Rate bound (I4): two ticks are ≥ one interval apart in stamp, because the step that consumes a slot is stamped ≥ that slot (H5) and ≤ `now + 132 s`
(lead cap), so `ticks ≤ horizon/interval + 2` for any mix — proven by extending ADR-0165's simulation with the three-way carrier.

## 5. Fold (virtual processor / `palw_state_v2`) — E3 only

* **FALLBACK credit** (step 4, after the clock): the block's own FALLBACK, then merged ones in consensus order; credited iff bond eligible
  (`palw_bond_may_take_work_v2`), floor state Idle at the block's slot, `FallbackBondSlot[bond] < slot`; effect `fallback_weight += w_fb`, delta 130/131.
  A refusal is a skip (merged) or a disqualification (own), as for an attempt.
* **E-BLUE credit**: an Exec member with verdict `EBlue` is credited iff the permit's claim is `Final`, `(span, round, index)` granted by the
  schedule, `(claim_id, round_index)` not in `ExecCredit`, the pool has a share; effect `safe_weight += per_round`, delta 132. `E_VOID` otherwise.
* **Claim weight split** at `Final`: `W_attempt = W_claim − pool` instead of `W_claim`, only when `σ > 0`.
* **Seed anchor** (ADR-0170 M1): admitted REAL attempts only past F; FALLBACK never records one.
* **Anchor duty**: `operator_of_v1` extended to the algo-12 envelope; the binder's seed is the operator attempt's execution commitment (Q6).
* **Comparator**: `PalwCandidateOrderV1::new(frontier, safe_weight + fallback_weight, immature, hash)` past F (Q11).
* Reorg: every write is a delta entry with an exact revert; state Some-only so a chain that never saw the rules roots as one without them.

## 6. Header / body stateless checks

* algo 12: envelope decodes, version, signature verifies under the embedded key, fixed `2^-24` puzzle, stamp ≥ `slot + G` (H3 analogue), lead cap.
* algo 8 with own DAA ≥ F: invalid by name (`RetiredHeartbeatLane`). Attempt-lane header declaring the BASE-0 class with DAA ≥ F: invalid
  (`RetiredBaseClass`) — the only read of a class at the header stage, and against the declarer.
* Exec lane: unchanged (ADR-0125), no new field, no header change.

## 7. RPC and audit

`blockKind` gains `FALLBACK`; `blockClass`: `C_BLUE | C_RED | E_BLUE | E_VOID | RED | FALLBACK | FALLBACK_RED`, derived from the recomputed verdict
plus the fold's acceptance (the audit index record, `audit/rpc-index`'s `block_accounting`). Explorer: useful-work ratio and REAL-carried tick ratio per
window; per-claim weight table.

## 8. Tests

Pure (stage 1, **built in the skeleton**, §9): lane mapping dormant/armed; colour rule table; F1 `Classic` for a non-hanging attempt; `peer_counts_v2`
over every (rule, peer) pair; **I1** `scoring_delta_v2` invariant under any number of E-BLUE members (property); **I2** a REAL's
colour-relevant peer set invariant under FALLBACK peers, and the exact boundary at the merge-depth floor; **I3** weight split bound for random
`(W_claim, σ, N, rounds)` and `σ = 0`, `N = 0`; **I4** the three-way carrier simulation (adversarial mixes of up to 100 sources, 300 rounds): DAA ≤ 1
per step, REAL + FALLBACK = +1, ticks ≥ one interval apart, horizon bound; carrier priority table; `fallback_stamp_admits_v2` boundaries; G = 0 is
ADR-0165's rule byte for byte (equivalence with `palw_clock_tick_source_v1`).

Next (stages 2–5; not built): pipeline tests on testnet-12's own GHOSTDAG — a slow REAL behind N FALLBACKs (BLUE on the chain, classic RED on a
withheld branch; the merge-depth edge), a 120-round claim (DAA, blue score, `safe_weight`), a second node fed the same blocks in shuffled orders, a
pruned join, the pruning proof across F, a reorg across F, dormancy byte-identity (same chains, fence never reached: same root at every block).

## 9. Stages

| stage | content | state |
|---|---|---|
| 0 | ADR-0172 + this spec | written |
| 1 | fence in the four places + `palw_accounting_v2.rs` pure functions + property tests; `t12-repin --drift-only` clean | **skeleton: see the as-built note below** |
| 2 | GHOSTDAG wiring (§3), predicates audit (Q14), pipeline colouring tests | not built |
| 3 | clock wiring (§4), carrier telemetry, simulation extended | not built |
| 4 | fold: FALLBACK credit, E-BLUE credit, weight split, anchor duty, comparator, state/delta/carriage | not built |
| 5 | stateless checks, RPC, audit index, producers (FALLBACK miner replaces the heartbeat miner and the floor producer), drill scripts | not built |
| 6 | drill (ADR-0172 §8), then the lead decides F | after the 5,300 release |

**As-built note (stage 1, 2026-10-04):** `consensus/core/src/palw_accounting_v2.rs` holds `lane_v2`, `colour_rule_v2`, `peer_effect_v2`,
`exec_verdict_v2`, `scoring_delta_v2`, `palw_clock_carrier_v2`, `fallback_stamp_admits_v2`, `claim_weight_split_v2` / `claim_weight_bound_holds_v2`, the
companion values, and `Params::{palw_accounting_v2_fence, palw_accounting_v2_active_at, validate_palw_accounting_v2}`. The fence is written in the
places the repo's pattern asks (field; `palw_fences_v1`; Some-only in both ids; the `never()` collapse; the fork visit; the four preset literals;
the fork-id probe arm; `validate_palw_v2`, last). No mirror in the V2 bundle (nothing folds it yet). **Nothing reads the fence except the
validators.** The mutual-exclusion lines for `palw_exec_class_v1` and RFC-0008's three fences are not written: those fields live on other
branches (`rcore/exec-class`, `rfc8/claim-backed-blocks`); integration adds them. Tests: 11 unit tests (lane map; dormancy; colour table; FALLBACK
invisibility and its property over fallback counts; **I1** round count moves no scoring quantity; carrier priority; equivalence with ADR-0165's
rule at G = 0 over 5,000 random facts; **I4** the three-way clock simulation, 300 rounds at G = 0 and G = 20 s; **I3** the weight split over 3,000
random claims) and 3 integration tests (`palw_accounting_v2_fence`: dormant on every preset, armed moves params and schedule ids and never the identity,
fork-id gated, refused without each prerequisite). `scripts/t12-repin.sh --drift-only`: no drift (params `5ee7fd8e…`, schedule `1678e073…`).

## 10. What this spec does not decide

Every Q of ADR-0172 §10. The skeleton's `palw_accounting_v2_value_v1()` carries the recommended defaults (G = 20 s, σ = 0) and nothing reads it
past the validators, so no default can leak into a rule before a decision is taken.
