# G14 lane rfc08-hold — RFC08 / ADR-0184 §1 fee hold for text29 and funded v11 (2026-10-22)

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/rfc08-hold`, base `494ef1e5e`.

## Fence
Reuses the existing dormant `palw_native_liability_horizon_v1` (ADR-0184 実装記録 2026-10-18; requires `palw_settlement_once_v1` at or
below it). No new fence. Arm both for t12 at DAA 16000 centrally. Only claims **committed** at/after the fence hold
(`native_fee_needs_hold_v1(params, committed_daa, final_daa, signed_deadline)`), H = signed deadline + `window_court`
(`native_horizon_v1`, the same formula as RFC08 08-A `native_claim_hard_deadline_v1`) — job39's helpers, reused unchanged.

## Implemented
Shared (`consensus/core/src/palw_native_liability_hold_fold_v1.rs`):
- `native_held_dispute_open_v1` — family dispatcher for the closing-tick sweep (job39 → existing `native_dispute_open_v39`;
  text29 / funded11 → new per-family fns; unknown family → open, never paid). `palw_kernel_route_fold_v1.rs` tick now calls it
  (before, every non-39 row was "open" forever, so no text/funded row could ever release).
- `after_held_fee_left_v1` (private) — release/refund side effects dispatched by family (job39 → `clear_held_producer_slot_v39` as
  before; text29 → close the quota subject; funded11 → nothing).
- `native_refund_queue_of_v1` — refund queue from parts (text29 collection re-derives it after the row is gone).

Text29 (`palw_native_text_fold_v29.rs`):
- `settle`: past the fence a producer Final writes an aux-144 held row (queue = existing `terminal_queue`, budget =
  `native_text_budget_id_v29`, refund payee = requester payout) instead of `pay_native_recipient_v1` + `write_payout`; quota subject
  stays open; budget reservation stays open. Credit row (s, queue, daa) written as before. Below the fence: identical writes/order.
- `court`: a Final with a held fee inside H is accusable by **public filing only** (`seal == None`); uses the existing
  `NativeTextBookV29::convict_after_final(.., horizon)` and new `settle_after_final` (refund via `refund_native_fee_v1`, slash the
  collectible reserve, pay finder as `settle`, replace credit row with refund queue).
- `collect_terminal`: refused while a held fee exists (material retained to H); accepts the refund queue for a post-Final conviction.
- `held_dispute_open_v29`, `held_fee_left_v29`.

Funded v11 / Final27 (`palw_flat_execution_fold_v11.rs`, `..._court_fold_v11.rs`, `..._gc_fold_v24.rs`):
- `credit`: past the fence a Final's producer credit is held (queue = `native_final_credit_queue_v27`; refused if the job has no
  Final27 right, since a pooled payout can't be held); budget `funded_execution_budget_id_v13` not closed. `close_adjudicated_quota`
  still runs at Final (quota accounting unchanged).
- `court::file`: accepts a held Final within H (work fee prepaid; no quota public-action debit since the subject closed at Final);
  `convict(.., held_horizon)` uses the existing kernel `ExecutionJobStateV11::adjudicate_held_final` and new
  `credit_after_final_v11` (refund, finder, job/credit row replaced). Reveal path passes `None` (unchanged).
- GC `terminal_header`: refused while the fee is held.
- `funded_held_dispute_open_v11`, plus wrappers `text29_held_dispute_open_v29` / `text29_held_fee_left_v29`.

## Tests (consensus-core, `palw_state_v2::tests::native_liability_hold_v1`)
- `rfc08_hold_text29_and_funded11_are_paid_exactly_past_the_horizon`
- `rfc08_hold_text29_and_funded11_convicted_within_the_horizon_refund_the_requester`
- `rfc08_hold_refund_queues_are_per_family`
- `rfc08_hold_the_dispute_dispatcher_fails_closed_on_an_unreadable_job`
- `rfc08_hold_below_the_fence_text29_and_funded11_keep_the_immediate_payout`
Run: `cargo test -p kaspa-consensus-core rfc08_hold` (crate name per workspace). These are hold-fold level; **no end-to-end
text29/funded fold test** (Final → hold → filing → refund + slash) was written — the fixtures for a full text29/Final27 job are not
available at unit level here. Producer slash on post-Final conviction is exercised only by code review; needs central E2E.

## Not done / open
1. **Post-Final DA** (text29 public DA, funded v11 demand/settle) is NOT reopened during the hold — only public court filing.
   Final already requires no open demand/owner, so nothing pending is lost, but a *new* DA request after Final is refused
   (DA clocks are bounded by `retention_until`/deadline, and capacity planning would need extending to H). "default → refund" is
   therefore only reachable pre-Final (old path, which already refunds). Needs a design pass if post-Final DA is required.
2. **Mining39: not changed.** Receipt quanta pay issuance + fork-choice weight in the receipt block's coinbase — there is no
   requester fee to hold or refund. The escrow-rfc08 lane's `NativeClaim` leg (H = `palw_native_mining_liability_until_v39`) already
   gives the H source; that H is ≥ receipt `expires` (admitted + cert lifetime + receipt lifetime + prosecution), so "pay at H" would
   make every receipt unspendable. Existing G-N2: collect waits for H, conviction claws back granted weight. Deferring
   issuance/weight to H is a design change → **user decision** (ADR-0184 §実装記録 already lists it as 残り). No overlap with
   escrow-rfc08's lines (mining fold untouched by this lane).
3. Text29 held Final keeps `paid_status.active = false` (as before); post-Final owners (paid seals) are not admitted.
4. Funded v11 post-Final filing pays only the work fee (no per-job public-action quota debit) — spam bounded by the prepaid work fee.
5. Audit readers (`audit_record_from_rows_v29`, Final27 audit) may need to understand a Convicted record whose queue is the
   refund queue; not reviewed.
6. Merge overlap: `palw_native_liability_hold_fold_v1.rs` release/refund now call `after_held_fee_left_v1` (was a direct
   `clear_held_producer_slot_v39`); `palw_kernel_route_fold_v1.rs` tick closure changed.
