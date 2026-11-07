# G14 lane r5-clock-deps-relay (handover §5 item 4) — 2026-10-22

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/r5-clock-deps-relay`, base 64bc87dcd.
Fence: reuses `palw_court_clock_v2` (court-clock lane) everywhere; Gate C fence for the receipt pause. No new fence.
**Arm centrally:** `palw_court_clock_v2` (>= `palw_legacy_public_filer_v1`); arming is now ALSO refused if T06 fails (item 6).

## 1. Other court routes switched to clock v2
- `consensus/core/src/palw_court_clock_v2.rs::palw_route_collect_until_v2 / palw_route_door_open_v2`: governed claim (accepted >= fence)
  -> door = `hard_v2 + 1` (`a+3,121` undisputed; `a+10,001` once the dispute record exists), never shorter than the route's signed
  door; not governed / no claim row -> the signed door byte for byte.
- Wired: `palw_staged_court_fold_v1.rs` all 6 owner-bundle door checks (history recovery, selected source, native selected source,
  TIR prefix move, flat constraint source, Float32 source — i.e. the prefix/tokenizer/constraint/float/history/selected routes,
  which reach the fold through these bundles); `palw_tir_public_fold_v1.rs` `apply_conviction_v1` and `apply_witness_part_v1`.
- NOT switched (deliberate): the inner owned-court state machines (`palw_prefix_owned_court_v1`, `palw_tokenizer_owned_court_v1`,
  `palw_constraint_stream_owned_v1`, `palw_flat_constraint_stream_v1`, `palw_history_custody_v1`, `palw_tir_public_owned_v1`) keep
  their structural `stream <= collect <= material` checks and own `now >= collect_until` turn checks — they have no params; the fold
  door above gates them. Liveness reads `palw_state_v2.rs:~2829`, `palw_legacy_public_filer_fold_v1.rs:~457` still use the signed door.
  Shared court rows (CourtShared*) carry no own door (they read the original hard via the frontier) — unchanged.

## 2. X3 × clock v2
- `palw_ruling_fences_v1.rs::palw_claim_pursuit_may_start_v1` delegates to `palw_court_clock_v2::palw_claim_pursuit_may_start_v2`
  when clock v2 governs the claim: cutoff = ADR-0198 A notice window `a <= start <= a+3,120`; finish line = `a+10,000`; budget =
  X3's `B_remaining` when X3 is armed for the claim, else 0: `start + B_remaining <= a+10,000`. X3's own policy hard/cutoff are not
  read for a v2-governed claim. Rationale: RFC-0014 2026-10-19 amendment (3,120/10,000 from original acceptance) is the adopted clock;
  X3's "fixed at acceptance" still holds since a+10,000 is fixed at acceptance.
- TIR install check uses `palw_claim_route_hard_v2` (v2's a+10,000 when governed, else X3's hard).
- Retention: write_claim takes max(X3 retention, v2 retention) — consistent.

## 3. Dependency contracts (F07)
- `palw_court_clock_v2_dependencies_cover_v2` (kinds ClaimTrace/Exec/Provider/Custody): every horizon must be >=
  `a + 10,000 + max(W_disclose, reorg_retention)`; else typed `TooShort(kind)`.
- `palw_dispute_record_hard_deadline_v2` now takes `reorg_retention` + `dependencies`; the extension (dispute record creation in
  `palw_legacy_public_filer_fold_v1.rs` apply reservation) is refused atomically (nothing written yet) if any is short. Wired deps:
  the claim's own trace + the staged route's `material_until_daa` (Custody) when a staged claim contract exists.
- Reorg term from params: `palw_court_clock_v2_reorg_retention(extras.staged_public_court)` (config `reorg_retention_daa`, 256 on
  `network_v1`; 0 where the staged court is not in force). `TransitionBuilder::write_claim` now raises governed claims' trace retention
  with that term too.
- OPEN: EXEC worker and provider lease horizons are not state fields reachable at the reservation (no lease record in
  `PalwClaimStateV2`); the kinds exist and the caller list is where they plug in. **Consequence to decide:** with `network_v1` the staged
  route's material (`a + acquisition 4,096 + plan + collection 4,096 + 256`) is likely < `a+10,256`, so a governed staged claim's dispute
  extension is REFUSED until the staged config's windows (or route material at admission) cover a+10,000 — user/param decision.

## 4. Current signing authority
- `palw_court_clock_v2::palw_court_current_signer_key_v2(state, actor)`: the bond's CURRENT pubkey at the response DAA; Active and
  Retiring both answer (Retiring backs claims to resolution, ADR-0042 D6 item 9); unregistered -> refused. Used by
  `palw_court_shared_tir_response_v1::SignedSharedDataResponseV1::validate_bond_signature`. Other shared-court checks
  (`*_frontier/root/round/stream`, staged fold `CourtShared*` rows) already read `state.bond(signer).pubkey` at fold time (current key),
  no status filter — semantics already match; not rewritten.
- Designated court-signer key: NOT implemented (would change `PalwBondStateV2` borsh layout / needs a versioned record — design
  decision per RFC-0026 §6 "separate spec").

## 5. Relay + receipt pause
- Relay: signed messages name no carrier; signature binds network/claim/scope/request id/signer/query block+DAA/digest; rows are
  write-once. Test proves relayed bytes verify and replay to another request id / query DAA is refused.
- Receipt pause (user decision): `PalwReceiptPauseV2 {open_since_daa, paused_total_daa}` + `palw_fp_receipt_window_start_v2` +
  `palw_rfc0018_v1::palw_fp_receipt_clock_paused_v2` (suspended -> Err "accusation-pending"; after dismissal start += paused total, so
  the remaining window resumes). **OPEN:** the per-claim pause account is not persisted — needs a rooted row (e.g. a new key prefix in
  kernel aux table 136 or a staged-court row) written at accusation open/dismissal, and the 8 callers of `palw_fp_receipt_clock_v1`
  (admission, fold ReceiptSpend, processor pre-check ×2, RPC, kaspad e2e) switched to the paused variant. Not done: requires a new
  state row + generated scanner regen.

## 6. T06
- `palw_court_clock_v2_t06_v2` (`turns·turn + evidence + recording + recovery + reorg + 1 <= 6,880`; exact boundary 6,879 ok / 6,880
  refused) and `palw_court_clock_v2_t06_of_params_v2` (X3's B_remaining when armed, else W_disclose + window_court). Wired into
  `Params::validate_palw_court_clock_v2` (`palw_g14_gates_v1.rs`). Risk: arming with X3's probe policy may now be refused if its
  B_remaining + 257 > 6,880 — that is the intended mechanised T06.

## Tests (crate `kaspa-consensus-core`, lib)
- `palw_court_clock_v2::tests::{r5_every_dependency_kind_must_cover_the_extension_minus_one_equal_plus_one,
  r5_reorg_retention_term_comes_from_params_and_dominates_when_larger, r5_the_extension_is_refused_atomically_by_a_short_custody_dependency,
  r5_route_doors_switch_to_clock_v2_for_governed_claims_only, r5_x3_and_clock_v2_combined_start_rule, r5_t06_residual_6880_boundary,
  r5_receipt_window_pauses_during_suspension_and_resumes_on_dismissal}`
- `palw_court_shared_tir_response_v1::tests::r5_court_response_uses_the_current_key_retiring_answers_and_a_relay_carries_it`
- updated call sites in `tests/court_clock_v2_fold.rs` (new args `0, &[]`).
Run: `cargo test -p kaspa-consensus-core --lib r5_` and `cargo test -p kaspa-consensus-core --test court_clock_v2_fold`.
No fold-level integration test was added for the route-door switch (needs a staged bundle fixture past the fence) — central E2E.
