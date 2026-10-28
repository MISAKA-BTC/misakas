# G14 lane court-clock (U06/U07) — 2026-10-22

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/court-clock`, base 8b036ed38.

## Fence
New dormant `Params::palw_court_clock_v2: Option<ForkActivation>` (X3's `palw_claim_hard_deadline_v1` was NOT reused: X3 says
"fixed at acceptance and never extended" and carries an unapproved policy; RFC-0014 2026-10-19 head says the extension rule comes in
"a new versioned fence", old claims keep the old ruleset). `None` on all 4 presets; hashed Some-only (never() collapsed) in both
identity fingerprints (`config/params/identity.rs`); accessor `palw_court_clock_v2_fence()`; mirror `court_clock_v2_from_daa`
(`#[borsh(skip)]`, `palw_state_v2.rs` + `state_params.rs`), written by `sync_palw_g14_gates_v1`; validation
`Params::validate_palw_court_clock_v2` (`palw_g14_gates_v1.rs`, called from `validation.rs`): needs `palw_legacy_public_filer_v1` at
or below it + mirror agreement (same pattern as `palw_court_outsider_lane_v1`; LG14-A's fence carries the arming record).
**Arm centrally:** `palw_court_clock_v2` (>= `palw_legacy_public_filer_v1`). Not added to `fork_id_v1.rs` name table — add if the
central arming tool sets fences by name.

## Implemented
- `consensus/core/src/palw_court_clock_v2.rs` (new): `palw_court_clocks_v2` (all clocks via `CourtClockBounds::derive`; undisputed hard
  = a+3,120, extended = a+10,000; collect_until = hard+1 exclusive; retention = hard+W_disclose; bond release = max(retention, other
  liability)), `palw_court_clock_v2_governs` (accepted_daa >= fence: T15), `palw_claim_court_hard_deadline_v2` (the read used by
  every fold site; old formula when not governed), `palw_claim_court_reserve_hard_v2` (capacity provisioning = a+10,000),
  `palw_dispute_record_hard_deadline_v2` (the extension; first reservation must satisfy ADR-0198 A `a<=d<=a+3,120`),
  `palw_court_clock_v2_required_retention`.
- U07 extension = creation of the LG14-A dispute record (`palw_legacy_public_filer_fold_v1.rs` apply_dispute_reserved): its
  `hard_deadline_daa` = a+10,000; DL-1 disarm (Final hold), `da_rekey_v1` (vesting/locks, bond hold via `until = hard + W_challenge`),
  the lapse sweep queue (indexes `record.hard_deadline_daa`), disclosure/retention reads — all derive from that one rooted write and
  revert with it. Per-turn deadlines are not touched. Record re-created after deletion still measures from `a` (no reset).
- Read sites switched: legacy filer fold (reacquire, claim facts, filer candidates, in-window), staged court fold (3 sites, paid stage
  retention no longer moves a governed claim's hard), capacity fold (admission bucket and in-flight hard = a+10,000 for governed).
- F07 (RFC-0026 §3.3): `TransitionBuilder::write_claim` raises a governed claim's `trace_retention_daa` to >= a+10,000+W_disclose at
  creation (same mechanism as X3 REQ-X3-6). Other dependency contracts (EXEC worker, provider lease, kernel liability, custody) are
  NOT checked here — remaining.

## Tests (written, not run)
- unit `kaspa-consensus-core` lib `palw_court_clock_v2::tests::{court_clock_v2_undisputed_finishes_at_the_normal_window,
  court_clock_v2_extension_moves_every_clock_together, court_clock_v2_boundaries_3120_and_10000,
  court_clock_v2_signed_retention_must_cover_the_maximal_extension, court_clock_v2_other_bond_liability_holds_only_the_bond,
  court_clock_v2_overflow_is_refused}`
- `cargo test -p kaspa-consensus-core --test court_clock_v2_fold`: `court_clock_v2_validation_needs_the_filer_and_the_mirror`,
  `court_clock_v2_unarmed_is_byte_identical`, `court_clock_v2_a_dispute_extends_every_clock_atomically_and_reverts`,
  `court_clock_v2_notice_3120_and_hard_10000_boundaries`, `court_clock_v2_an_old_claim_keeps_the_old_clock_across_the_fence`.
  Reorg: every `Chain::step_at` re-applies/reverts the delta and reloads the carriage; the extension test also checks the parent.

## Open / uncertain
- Interaction with X3 when both are armed: v2's helper takes precedence at the switched sites, but X3's
  `palw_claim_pursuit_may_start_v1` still runs at reservation. Needs a decision before arming both.
- Other court routes (TIR public fold, staged bundle, prefix/owned courts, shared TIR state) carry their own `collect_until` /
  `material_until` from their contracts; not switched (they don't read `trace_retention − W_disclose`). RFC-0026 §2.2 boundary table
  for those remains.
- Reorg retention term = 0 on the legacy route (no such config there); the 6,880 residual-budget check (T06) is not mechanised.
- Fold tests assume the floor claim stays ReceiptLicensed to a+10,000 under the hold (as LG14-A's test does for the legacy hard).
- court-f02 lane had no commits past 494ef1e5e when read; no shared functions edited by design, but both touch
  `palw_legacy_public_filer_fold_v1.rs` — watch merge.
