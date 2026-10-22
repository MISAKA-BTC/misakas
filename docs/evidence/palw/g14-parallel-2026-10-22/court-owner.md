# G14 parallel lane: court-owner (2026-10-22)

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/court-owner`.

## Fence

New dormant fence `Params::palw_court_outsider_lane_v1: Option<ForkActivation>` (`None` on every preset; hashed Some-only in both
identity fingerprints, `config/params/identity.rs`). Accessor `palw_court_outsider_lane_v1_fence()`. Mirror on the V2 state params:
`PalwStateParamsV2::court_outsider_lane_from_daa` (`#[borsh(skip)]`, like the other G14 mirrors), written by
`Params::sync_palw_g14_gates_v1`; reader `court_outsider_lane_active_v1(daa)`.
Validation `Params::validate_palw_court_outsider_lane_in_v1` (called from `validate_palw_v2` in `config/params/validation.rs`):
needs `palw_legacy_public_filer_v1` (LG14-A) in force at or below it, and the mirror synced. It needs no record of its own: LG14-A,
its prerequisite, is the record-gated fence.

**To arm centrally (testnet-12 @ 16000):** `palw_court_outsider_lane_v1 = Some(16000)` with LG14-A at <= 16000, then
`sync_palw_g14_gates_v1()`. Not added to `PalwG14GatedFenceV1::ALL` (would change that array and its arms); not added to
`fork_id_v1.rs`'s probe name map. Central step may want both.

## Task 1 (G14 ②, G-07 residual)

`consensus/core/src/palw_court_capacity_fold_v1.rs`:
- `palw_court_outsider_lane_followon_cap_v1(cap) = ⌊(cap−1)/2⌋` (re-exported from `palw_state_v2`).
- `legacy_court_cap_admits_v1(state, params, claim, now)`: replaces the two inline copies of the int-12 cap check in the
  `PalwCourtAdmissionV1::Legacy` arms of `palw_state_v2/objects.rs` (CourtOpened) and `palw_state_v2/da_sessions.rs`
  (dissection at named leaf). Below the fence: identical (`open >= cap` → `CourtSessionsAtCapacity`). Past it: same cap; additionally
  a **follow-on** session (its claim already has a running session) is refused once follow-ons hold `⌊(cap−1)/2⌋` of the cap.

Bound / deadline argument: total concurrent sessions are still `<= palw_max_concurrent_court_sessions_v1(turn) = turn × 1`, the same
number, so the per-block court slot's rung guarantee (audit A M-1) is unchanged — the cap was not raised, only partitioned. At least
`cap − ⌊(cap−1)/2⌋ >= 1` (≈ half) of it is room only for first sessions of distinct claims. Stacking Sybil dissections on one
decoy claim (the G-07 residual) can no longer fill the cap. The rule reads no owner, order or identity — only which claims have
running sessions — so it is not a first-come exclusive occupancy (ADR-0199); within each partition it is the old rule.

Residual (honest): (a) a coalition can still fill the first-session room with `≈cap/2` DISTINCT disputed claims of its own (each a
licensed claim; each decoy accusation that fails costs its accuser the claim reserve) — a full guarantee for that needs the bounded
claim admission + deadline queue of `palw_court_capacity_v1` (RFC-0025 draft parameters, not adopted, untouched here). (b) If a
colluder holds the target claim's first session, the outsider is a follow-on and competes in the follow-on room. (c) A
direct/one-move accusation that opens a session as a follow-on can now be refused when the follow-on room is full (only past the fence).

Tests (`consensus/core/tests/g14_close_legacy.rs`, target `g14_close_legacy`):
- `g07_residual_the_network_court_cap_is_shared` — unchanged; still pins the refusal below the lane fence (LG14-A armed, lane not).
- `g14_f03_outsider_court_lane_opens_past_decoy_occupancy` — new: lane armed; Sybils retry one block each (continuous refill),
  exactly `⌊(cap−1)/2⌋` admitted, the rest refused `CourtSessionsAtCapacity`, total never > cap, cap value unchanged; the outsider's
  session on the lying claim opens and its reservation still holds. (Helper `held_chain_lane`.) Does not drive to a verdict.

## Task 2 (G14 ④, owner slots)

TIR public frame (`consensus/core/src/palw_tir_public_fold_v1.rs`): `tir_owner_fair_start_v1` is now also true past
`palw_court_outsider_lane_v1` (previously only past `palw_court_capacity_v1`). This turns on, without any court-capacity policy, the
already-implemented owner queue (Open at K_owner waits instead of being refused), tenure rotation (an admitted row past
`palw_tir_owner_tenure_v1` yields while someone waits — deadline-based preemption of an owner not finishing, no forfeit) and idle
reclamation (deposit burned). K_owner = 8 unchanged; custodian load stays <= 8. Conviction filing (direct accusation with held
evidence) does not pass through the owner gate and is unchanged. Assumption to verify at build: nothing else on the fair-start path
reads the court-capacity policy (grep found only `tir_owner_fair_start_v1`'s three call sites).

Tests (`kaspa-consensus-core` lib, module `palw_tir_public_fold_v1::tests`):
- `g14r2_eight_active_colluding_owners_cannot_lock_out_a_newcomer_who_convicts_in_time` — now a wrapper (court-capacity mirror).
- `g14_f03_outsider_lane_eight_active_colluders_cannot_lock_out_a_newcomer` — new: same scenario with only the lane mirror: 8 active
  colluders + 8 later refill Opens, outsider queued, admitted at a tenure, convicts inside the collection door; below-fence refusal
  asserted inside the shared body `eight_active_colluders_newcomer_convicts_v1`.

**Native text29 lifetime owner catalogue (cap 4): NOT implemented.** The cap is not one counter but four coupled lifetime structures:
`EXECUTION_SEAL_ACTOR_TABLE_V11` kind 0 list (read with a 4+4×64 byte cap — a 5th entry would read as *empty* via `unwrap_or_default`),
`NativePagedJobStatusV29::owners_admitted` (lifetime u8, refused at >= 4 in `palw_native_paged_quota_v29.rs:37`, shared with namespace
28 and in `generated.rs`), `NativePhysicalSlotsV22::owners[4]`, and collection (`palw_native_text_fold_v29.rs` ~1978/2172) which
deletes each lifetime owner's 29u8/26u8 rows by iterating that list (dropping an expired entry would orphan rows). A v39-style
expired-owner reclamation needs: a fenced live count in the quota check, slot `retire_owner` at expiry, and a separate archived-owner
list for collection — a borsh/generated-layout change outside this lane's no-generated-files rule. Recommend a follow-up lane or
routing text29 outsiders through the claim-common frontier (ADR-0192) instead of an owner slot.

## Scope citations
ADR-0192 (owner-independent frontier, advance reservation), ADR-0199 (no first-come exclusive occupancy), RFC-0026 F03. No RFC-0025
parameter adopted; the ½ partition is structural (derived from the existing cap), flagged for user review.

## Round 2 (2026-10-22) — written, not compiled, not run

All behind the existing `palw_court_outsider_lane_v1` (no new fence; same arming as above).

### 1. Direct evidence-carrying accusations are exempt from the follow-on share
`legacy_court_cap_admits_v1(state, params, claim, opener, direct_evidence, now)` (`palw_court_capacity_fold_v1.rs`): when
`direct_evidence` it returns after the overall cap check — exactly the pre-fence rule (`open >= cap` refuses), so nothing admitted
below the fence on that route is refused past it. `open_dissection_at_named_leaf_v1` gained a trailing `direct_evidence: bool`:
- `true`: the IR / generative cone accusations (`TirShardCourtAccused`, `GenShardCourtAccused`, objects.rs) — their proof is the
  whole refutation (tags 62 / 88).
- `false`: held attention accusation `ShardCourtAccused` (`open_held_dissection_v1`; the decoy route of the G-07 test, argued in the
  dissection), held leaf challenge (`palw_held_close_fold_v1`), `CourtOpened`.
Design decision (flag for review): "direct accusation that carries its evidence" = the cone accusations. If the user means the held
attention accusation too, the follow-on share covers only `CourtOpened` + leaf challenges and the G-07 decoy test no longer holds.
Residual: exempt sessions still count toward the follow-on tally and the cap, so a coalition filing cone accusations on its own
claim can occupy the cap (each such filing carries a full refutation that must verify, and a failed one costs the accuser).

### 2. A colluder's first session does not demote the outsider
Identity read: `PalwCourtSessionStateV2::challenger_bond` (the only opener identity the state records). Inside side of a claim =
`claims[claim].bond` (producer) ∪ `panels[claim].seats[].bond`; anything else is outside. A new session is a follow-on iff the claim
already has a running session from the SAME side (`court_outsider_lane_is_followon_v1`); the tally counts every session beyond the
first of each side per claim (`court_outsider_lane_followons_v1`, ≤ 2 firsts per claim). Cost: a decoy claim can now hold 2 first
sessions (one inside, one outside) instead of 1 — still bounded by distinct claims.

### 3. Text29 lifetime owner catalogue reclamation (cap 4 kept)
`palw_native_text_fold_v29.rs::own`: past the fence a full catalogue (`EXECUTION_SEAL_ACTOR_TABLE_V11` kind 0) may evict the first
(catalogue order) member that `text29_owner_reclaimable_v29` accepts: archival seal retired (`expire_owner` ran), past its horizon,
NO progress (`text29_owner_made_no_progress_v29`: no part attempt, accepted part, assembly, court move, conversion), its caller row
26 retired with nothing remaining, no open public request. On eviction: `status.owners_admitted -= 1` (the token's `>= 4` check then
reads live-plus-progressed, as v39's `admitted`), the member leaves the catalogue, and its `(29u8, job, id)` / `(26u8, job, id)`
rows are deleted at once (what collection kind 2 would delete; nothing orphaned). Physical slots already free at `retire_owner`;
`paid_owners` already zeroed by `expire_owner`. No struct/borsh/generated.rs change needed.
Not done: namespace 28 (`palw_flat_stage_fold_v28.rs` table 19 catalogue, line ~224) keeps its lifetime cap — same mechanism would
apply; left for a follow-up. Uncertain: whether any audit/redemption path reads the evicted owner's archival seal (29u8,job,id) after
eviction (grep found collection and expiry only).

### Tests added (round 2)
- `kaspa-consensus-core` lib, `palw_state_v2::tests::rcore_s1_api::court_outsider_lane_round2::`
  `g14_f03_r2_a_direct_evidence_accusation_is_exempt_from_the_followon_share`,
  `g14_f03_r2_a_colluder_first_session_does_not_make_the_outsider_a_followon`.
- `kaspa-consensus-core` lib, `palw_flat_execution_fold_v11::...::palw_native_text_fold_v29::g14_f03_owner_reclaim_tests::`
  (filter `g14_f03_r2`): `g14_f03_r2_four_stalled_owners_yield_a_slot_to_an_outsider_after_the_deadline`,
  `g14_f03_r2_progressing_owners_are_never_evicted`, `g14_f03_r2_below_the_fence_the_catalogue_is_unchanged`.
The text29 tests cover the pure admission + progress predicate; the full `own()` path with eviction has no fixture (needs E2E).
