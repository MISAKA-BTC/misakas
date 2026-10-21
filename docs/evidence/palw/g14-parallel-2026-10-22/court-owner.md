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
