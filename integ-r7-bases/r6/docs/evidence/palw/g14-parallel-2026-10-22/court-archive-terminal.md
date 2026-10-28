# G14 parallel lane: court-archive-terminal (2026-10-22)

Status: **written, not compiled, not run** (lane rules: no cargo). Branch `g14p/court-archive-terminal`, base `494ef1e5e`.

## Task 1: RFC-0030 / ADR-0197 A / ADR-0201 S1, pruning a resolved archive

Already on base (not re-implemented): S1 timely discharge. `ArchiveDeclarationProgressV1` (key25/row31) and `apply_native_archived_chunk_v1` record the actual native point of each chunk and a once-only `completed` record. `archive_pending_nominal_v1` subtracts a qualified side's deposit, so `court_archive_holds_bond_v1` releases the exit hold. Late material never qualifies (tests `archive_s1_does_not_credit_an_unclocked_middle_chunk_or_release_the_other_side` and `rooted_archive_v5_replays_signed_bytes_without_state_or_money_changes`). Archive append still cannot change `fulfilled_groups` or the contract (`check_material_extension`). Nominal deposits are never escrow, and no money moves anywhere.

Added in this lane:
- `palw_court_end_archive_v1.rs`: `PalwChainStateV2::court_archive_side_resolved_v1` (applied debit, fulfilled close, or rooted qualified S1 record) and `court_archive_fully_resolved_v1`. The second returns true only when every side is resolved AND both actors have zero nominal hold. Because of that, an Uncaptured or related ending, which still holds the court stake, is **never** prunable.
- `palw_court_archive_verify_v1.rs`: `TransitionBuilder::prune_resolved_court_archive_v1(id)`, private and not dispatched. It removes the progress rows, the signed receipts and the archive, in that order. It runs under a checkpoint with a scoped `court_archive_prune` flag, and restores on error. `archive_prune_removes_v1` lets the removal through only for this session's own three row kinds, and only while the pruner runs.
- `palw_staged_court_fold_v1.rs::write_staged_row_v1`: the receipt, archive and service-material-hold refusals, plus `check_archive_progress_write_v1`, are bypassed only for that scoped removal. All other removal or retype paths are unchanged and still refused.
- New builder field `court_archive_prune: Option<Hash64>` (`palw_state_v2.rs`, init in `transition_builder.rs`). It is not state and not wire.
- Removal is journaled as `StagedCourtSeed` entries, so undo/reorg restores every row exactly.

Test: `consensus/core/src/palw_tir_public_fold_v1.rs` → `archive_s1_timely_fulfillment_discharges_and_prunes_only_when_all_resolved` (kaspa-consensus-core lib tests). It covers:
- no prune while a liability is pending;
- a late chunk, and a late private append, neither discharges nor prunes;
- a timely chunk at the deadline discharges, releases the hold and moves no money;
- direct removals are refused;
- prune succeeds and is once-only;
- undo restores the parent root, and redo re-prunes.

Fence: the archive has **no DAA fence of its own**. It exists only while the court service book holds the claim (`write_court` → `court_service_holds_claim_v1`). The pruner uses that same condition: it requires `court_service_state_v1()`. The S1 writer and the pruner are both private and undispatched, so nothing changes on any network until a public paid ingress is installed (ADR-0201: not adopted).

Open questions:
1. Pruning also drops material whose `material_until_daa` may not have passed. I bypass `court_service_holds_material_v1` for these three row kinds of the pruned session only. If reviewers want material retention to also gate pruning, add `tip >= material_until_daa`. This is a user/ADR decision.
2. Pruning is not automatic. Someone must call it; no dispatcher or ingress exists.

## Task 2: RFC-0024 A, narrowing the terminal tile to one position

- `palw_tir_dissect_v1.rs`:
  - The fold check is factored into `check_children_fold`. Its behaviour is unchanged.
  - New pure, never-stored `PalwTirPositionNarrowingV1` with `PalwTirDissectPhaseV1::position_narrowing_v1`, `check_position_round_v1`, `choose_position_v1`, `terminal_position_range`.
  - It folds positions to the tile claim exactly (Σ/max). It evaluates against root totals, with no per-position softmax renormalisation (RFC-0024 §4).
  - A one-position tile needs no round.
  - No phase field, borsh layout or wire change.
- `palw_tir_court_v1.rs`:
  - `check_tir_dissect_bottom_v1` now delegates to `check_tir_dissect_bottom_range_v1`. Its behaviour is identical.
  - New: `check_tir_dissect_position_bottom_v1`, which evaluates `eval_demanded_range` over `[p,p+1)` only. Its canonical-used check means the carriage must be exactly that position's read set.
  - New: `build_tir_dissect_position_round_v1`, `build_tir_dissect_position_bottom_v1`.
  - New: `tir_dissect_terminal_narrows_to_position_v1(bounded_door, daa)`, the selector.
- Reuse: this is the generic-reduction analogue of 16458bbb1's `palw_tir_history_position_round_v1`. That function is A16-attention-specific (max/exp_sum/v_acc), so it cannot be called directly on TIR reductions. The TIR dissection's own fold and evaluator are used instead.

Fence: `palw_tir_bounded_door_v1` (ADR-0190 door; RFC-0024 A changes the door terminal). Arm it with the rest at testnet-12 DAA 16000.

Tests: `consensus/core/tests/palw_tir_dissect_position.rs`, run with `cargo test -p kaspa-consensus-core --test palw_tir_dissect_position`:
- `an_honest_terminal_narrows_to_each_position_and_acquits`
- `a_lie_moved_between_positions_is_named_and_convicted_at_first_middle_and_last`
- `below_the_door_fence_the_terminal_stays_the_whole_tile`

The fixture's `h_tile` is 2, so "middle" coincides with "last" there. The `picks()` helper covers a real middle position when the tile width is 3 or more.

Not done:
- The court close path (`palw_court_v2.rs:1671`) and the shared bottom (`palw_court_shared_tir_bottom_v1.rs:104`) still call the whole-tile bottom. Wiring the position round into signed moves needs new versioned objects (`PositionDissectionV2`/`PositionCloseV2` in RFC-0024 §4). That means a new wire, which this lane must not add.
- The `PositionOperandResolverV2` witness budget (`PositionWitnessBudgetIncomplete`) is not implemented.
- The gen/improve courts (`palw_gen_court_v1.rs:1164`, `palw_improve_eval_court_v1.rs:896`) are not adapted.
