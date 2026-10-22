# Lane rfc23-stage2 — RFC-0023 stage 2 checkpoint and its consumers (2026-10-22)

Status: **Rust written, not compiled, not run** (lane rule: no cargo). No node / drill was run.
Base `494ef1e5e`, branch `g14p/rfc23-stage2`. Fence: reuses the existing dormant `palw_claim_local_settlement_v1`
(no new fence, no new wire tag; one new aux key in the already-allocated claim-local table 144: prefix `'C'`).

## What was implemented

### 1. Fold-derived checkpoint (`consensus/core/src/palw_claim_local_checkpoint_fold_v1.rs`, submodule of `palw_state_v2`)
- `expected_claim_local_checkpoint_v1(state, params, point)`: builds the expected `PalwClaimLocalCheckpointV1` from the
  committed state at `point`. One `ChainClaim` record per live claim: acceptance DAA/blue, accepting chain block (the
  retained admission frame's point, else the carrying block), `anchors` (attempt), `execution_root`, `final_claim`,
  `certified` (= existing `approval()` on the claim's Gate C row is `Ready`), `voided`, `liability_closed`
  (Final: existing `live_source(.., retiring_self=false)` is `Ready` — own court/trace/EXEC windows, prosecution,
  native dependency; Voided: not in `unresolved` and no open court; else false). Open records carry material: the
  claim row and its court-session rows inline (`palw_liability_material_id_v1`). `liability_state` = borsh of the open
  set (claim id, claim row, its court sessions) + the accepting blocks of their retained admission frames (frames are
  already rooted rows of table 144). Empty when nothing is open.
- `write_claim_local_checkpoint_v1(builder, parent, ctx)` — called at the end of every block in
  `apply_palw_transition_v2_with_extras` right after `retain_changed_claim_context_v1` (`palw_state_v2.rs`). Validates
  its own value with the same `validate()` a reader uses (caller `expected` = the recomputation), writes it as aux row
  `144 / 'C' ‖ 0^64` (a `KernelRouteRow` delta entry → reorg reverts it exactly; in the state root and the
  pruning-point carriage), and sets the frontier (see §2).
- `assert_claim_local_checkpoint_v1` — hooked into `assert_claim_local_context_v1` (runs on carriage load / import and
  in `assert_internal_consistency`): stored row must equal the recomputation at the state's last point (records,
  liability_state, material); row missing past the fence or present below it is refused.
- `palw_claim_local_checkpoint_status_v1` (pub, re-exported from `palw_state_v2`) — validated per-claim read.
- `wire_kernel_aux.rs`: scan arm for the `'C'` row (`PalwClaimLocalCheckpointV1::scan_native_wire_v1`, which already
  existed in the private inventory).
- `palw_claim_local_rewards_v1.rs`: `frames`, `cert_bytes`, `approval`, `live_source` made `pub(super)` (no behaviour change).

### 2. Consumers — every `safe_frontier` reader, decided per reader
Design decision: past the fence the fold's single frontier (`safe_frontier`) **is** the claim-local irreversible
frontier, so fork choice, settlement and pruning's input cannot diverge (RFC-0023 §4.5 "change together"):
frontier = max(parent frontier, this block's irreversible anchor). Irreversible anchor = deepest attempt whose whole
checkpoint dependency closure is Final + certified + not voided + liability-closed, OR an attempt retired from Final
in this very block whose parent-checkpoint closure was credit-eligible and closed-or-retired-this-block (production
claims retire exactly when their terminal clock ends, so they are never seen live with `liability_closed`). The
shared-prefix / Gate C recompute earlier in the block is superseded past the fence. Below the fence: byte-identical.

| Reader | Decision |
|---|---|
| `palw_state_v2.rs` frontier step / `Frontier` delta | Kept; past the fence overwritten by the checkpoint step, `new_frontier` re-read after it so the delta carries the final value. |
| `chain_state.rs::candidate_order`, `processor/palw.rs::palw_candidate_order_v2_checked`, `processor.rs:3917/10731`, `palw_fork_authority_v2`, rule-E leaf v2/v3, `palw_fork_choice_commitment_v1` | Unchanged code: they read the frontier, which is now claim-local past the fence (fork choice). |
| `palw_settlement_v1.rs::palw_settlement_v1`, `consensus/mod.rs:1207` (RPC settlement), `native_settlement.rs:483` | Unchanged code (comment added in native_settlement): `settled = frontier_daa >= d` now follows the claim-local frontier, so B's effects settle at B's time while A is open (matches `PalwValidatedClaimCheckpointV1::settled`). |
| `processor/palw.rs::palw_pruning_point_allowed_v2` | Past the fence (only reachable when Gate C is off — the Gate C branch already uses the progress ceiling and returns first) additionally bounded by `palw_pruning_progress_ceiling_v1` (below the oldest unresolved claim / undecided certification): pruning stays conservative; A's block history is kept until A resolves; A's liability material additionally travels in the checkpoint row. |
| `transition_builder.rs:7619/7654` (round schedule seed uses frontier) | Unchanged: the seed reads whatever the frontier is; past the fence it is the claim-local one (consistent across nodes since it is folded state). |
| `processor.rs:6676` (frontier ≤ point check) | Unchanged; still holds (irreversible anchors are accepted at or below the point). |
| Lifecycle: claim Final / retirement / reward release / vesting | Final is per-claim already; reward release already claim-local (rfc23-12 `reward_readiness_v1`); retirement and vesting count Finals, not the frontier. No change. |
| RPC | `getPalwClaims` row: new `claim_local_settlement` (version 7, only emitted non-empty past the fence; proto field 40 `claimLocalSettlement`), from `palw_claim_local_settlement_v1` via `palw_producer_v2::palw_claim_local_settlement_label_v1`. Settlement RPC unchanged (follows the frontier). |
| Pruning/IBD | Row is in kernel-route aux → in state root and `PalwStateCarriageV2`; import runs `assert_claim_local_checkpoint_v1` against the recomputation. |

## Tests (crate `kaspa-consensus-core`, `--lib`, module `palw_state_v2::tests::claim_local_rewards_v1`; filter `rfc23s2_checkpoint`)
- `rfc23s2_checkpoint_row_is_written_each_block_and_binds_the_unresolved_set`
- `rfc23s2_checkpoint_frontier_passes_unresolved_a_when_independent_b_settles` (B retires at 148 → frontier = B's blue score > A's; settlement settled at B's DAA; C (shares A's root) not irreversible; candidate order key)
- `rfc23s2_checkpoint_below_the_fence_keeps_the_shared_frontier` (no row; frontier below unresolved A; a not-yet-reached fence gives the same state root)
- `rfc23s2_checkpoint_reorg_reverts_the_row_and_the_frontier`
- `rfc23s2_checkpoint_pruned_import_reproduces_and_refuses_a_forged_row` (carriage round-trip, next block identical, forged/missing row refused)
All existing tests of this module (fence at DAA 100) now also exercise the per-block checkpoint write, its
internal-consistency check and delta apply/revert through `step()`. `world_at` was split into `world_with` (params
override); its hold assertion is conditioned on the fence being active.

## Fences needed
- `palw_claim_local_settlement_v1` — **still refused by `validate_palw_claim_local_settlement_v1`** (left untouched:
  the existing test asserts it and node drills / §6 campaign are unmeasured). Central step must lift that refusal to arm.
- Requires RFC-0018 Gate C armed at or before it (`certified` uses `approval()`, which holds forever when Gate C does
  not govern — without Gate C the claim-local frontier never advances). `palw_unresolved_bound_v1` (stage 1) recommended.

## Open risks / uncertain
- Not compiled. Likely friction: borrow of `builder` inside closures in `write_claim_local_checkpoint_v1`; `if let` chains.
- Every block past the fence calls `ensure_route_header` and rewrites the row (cost ∝ live claims + open material); tests
  past the fence without kernel-route extras may now fail at `ensure_route_header`.
- Per-block recomputation in `assert_internal_consistency` (and once more on the parent when a claim retires) is O(claims × frames).
- `inputs` are empty: no tx-output → claim consumption map exists yet; dependency = shared non-zero execution root
  (+ reservations, which are never shared). Retired sources are not in records (the reward reader covers them via frames).
- Retired-this-block detection trusts a `Claim{old: Final, new: None}` entry; a conviction that removes a Final row in
  one entry without first voiding it would be misread (existing void path voids first).
- At the fence block the inherited frontier is kept; Gate C's conviction rollback no longer lowers it past the fence
  (irreversible anchors are past conviction, but an inherited pre-fence reversible frontier is not re-checked).
- Expected values in tests (B retires at 148, A still open there) follow the rfc23-12 fixture; first run may need tuning.
- §6 multi-node acceptance campaign, node drill crossing the fence: not done.
