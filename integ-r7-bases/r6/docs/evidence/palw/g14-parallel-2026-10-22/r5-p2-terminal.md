# G14 parallel lane r5-p2-terminal: RFC-0024 position terminal, lookahead and multiproof (2026-10-22)

Status: **written, not compiled, not run.** The lane rules forbid cargo here. Branch `g14p/r5-p2-terminal`, base `64bc87dcd`. This lane continues `court-archive-terminal.md`, Task 2 (RFC-0024 A pure position narrowing).

Scope: G14 item 7, P2, implementation part. Design authority: RFC-0024 §4 (A), §5 (B), §7 and §11; ADR-0190 (the door fence `palw_tir_bounded_door_v1`); ADR-0191 (2) (the position round). The multiproof uses hashes only and proves no computation (PRINCIPLES §8).

## 1. What was implemented

### 1.1 Versioned signed move objects (RFC-0024 §4/§5): tags 201–204

Tags 201–204 are the **lowest unused tags above 200** in the A-2 kind table at the base: 230 and 240–245 were already taken. All four are owned by the new lifecycle fence `PalwLifecycleKindFenceV1::TirBoundedDoorV1 = 20`, which resolves from `Params::palw_tir_bounded_door_v1`.

| tag | variant (`PalwConsensusObjectV2`) | body (`consensus/core/src/palw_tir_position_terminal_v2.rs`) | signer / context |
|---|---|---|---|
| 201 | `TirPositionDissectedV2` | `PalwTirPositionDissectionV2`: session, tile round, tile `[from,to)`, root/tile claim digests, one claim per position | claim bond, ADR-0082 responder ctx |
| 202 | `TirPositionClosedV2` | `PalwTirPositionCloseV2`: verdict, dissection plus responder signature, `PalwTirPositionChoiceV2` plus challenger signature, a dissection-bottom `PalwCourtVerdictProofV2` (Tir/Gen/Eval), optional `PalwStepMultiproofV2` | both |
| 203 | `TirLookaheadDisclosedV2` | `PalwTirLookaheadDisclosedV2` (`PalwTirLookaheadDisclosureV2`) | claim bond, responder ctx |
| 204 | `TirLookaheadChosenV2` | `PalwTirLookaheadChosenV2` (disclosure plus `PalwTirChoicePathV2`) | session challenger, challenger ctx |

Registered in `palw_lifecycle_objects_v2.rs`:
- the may-ride arms (signature presence; a close carries a non-attention dissection bottom and stays under `DEFAULT_MAX_CLOSE_BYTES`);
- the owner match;
- `PALW_LIFECYCLE_NEW_KINDS_V1`;
- the A-2 row `ObjectTags {201..204}`;
- the allocation `(201, 204, …, ["palw_tir_bounded_door_v1"])`;
- the fence enum, `ALL`, `params_field` and the `Params` resolution;
- the wire pins `(201, 0xf42a9ee61f5d6def)`, `(202, 0x0621a77518b81c8d)`, `(203, 0x0710d31c2ea14fab)` and `(204, 0x15481d7f346cf561)`. These pins were derived by `scripts/drills/a2u_new_kind_wire_pins.py` without a build. The Rust test `every_landed_kind_is_pinned_to_its_wire_form` is the authority; re-pin from its `[a2u-pins]` output if they differ.

Other match sites updated:
- `palw_heartbeat_carriers_v1.rs` (court moves: `true`);
- `processor.rs` and `kaspad/src/palw_panel.rs` (name strings);
- `palw_validate.rs` (routed to the court validator);
- the fold's `palw_fold_kind_in_force_v1`;
- the A2U test arming fn (`t12_a2u_new_kinds_uniform.rs`).

Messages are domain-separated:
- `misaka-palw/tir/position/dissect/v2`;
- `…/position/choice/v2` (binds the dissection digest, so a choice cannot be replayed against another round);
- `misaka-palw/tir/dissect/lookahead/v2` (session, round, disputed tile range);
- `…/choice-path/v2`.

### 1.2 Wiring the one-position terminal into the real court

- **Adjudication:** `palw_court_v2.rs::adjudicate_tir_position_close_v2`. It checks, in order:
  1. version;
  2. witness budget;
  3. `check_close_cost_v2`;
  4. session, narrowed leaf and `tir_dissection_v1` phase;
  5. `palw_tir_position_named_v2` (binding, exact Σ/max fold, challenger's position).

  Then it grades:
  - **TIR:** `check_tir_dissect_position_bottom_v1`. The binding is filled and checked as in the `TirDissection` arm, plus the optional multiproof under `binding.step_merkle_root`, which must carry the narrowed leaf.
  - **Gen:** new `palw_gen_close_v1::check_gen_dissect_position_bottom_v1`, which calls `palw_gen_court_v1::palw_gen_check_dissect_position_bottom_with_v1`. The whole-tile function was refactored into a shared `…_range_with_v1`; the old function's behaviour is unchanged.
  - **Eval:** new `palw_improve_eval_court_v1::check_eval_dissect_position_bottom_v1`.
- **Signatures:**
  - `check_tir_position_close_signatures_v2` checks the responder on the round and the challenger on the choice. Without the challenger signature, the submitter would pick the position and a responder could buy its own acquittal.
  - `check_tir_position_dissected_acceptance_v2`, `check_tir_lookahead_disclosed_acceptance_v2` and `check_tir_lookahead_chosen_acceptance_v2` cover tags 201, 203 and 204.
- **Acceptance:** `consensus/src/.../palw_validate/court.rs`, new arms for 201–204.
  - Below the fence they are refused by name (A-2).
  - Past it, the TIR/gen/improvement fences are checked, then signatures, then full adjudication. The declared verdict must equal the derived one, in either direction, exactly as for `CourtClosed`.
  - New accessor `palw_tir_bounded_door_v1_at` in `processor/palw.rs`.
- **Fold:** `palw_state_v2/objects.rs`. The fold runs only when `extras.tir_bounded_door_active` is set (the new extras field, set in `palw_accept.rs`); otherwise it is a strict no-op, as for tag 245.
  - **202:** re-enters `apply_object_inner` as `CourtClosed { session_id, verdict, proof }`. There is **one close path**: the same conviction and acquittal, held-forfeit and session removal.
  - **201:** re-checks the binding against the terminal phase; it writes no state.
  - **203:** `apply_lookahead_disclosure_v2` writes the reconstructed level-1 round as the phase's pending round. The responder's clock is satisfied exactly as for a V1 round.
  - **204:** `apply_choice_path_v2`.
- **Position witness budget:** `PALW_TIR_POSITION_WITNESS_BUDGET_V2: Option<u64> = None`, with `check_tir_position_witness_budget_v2`. While the value is `None`, every position close is refused as `PositionWitnessBudgetIncomplete` (RFC-0024 §4.4). Over the bound it is `PositionWitnessOverBudget`. Both are refusals of the move, never a slash (§11 A-06).

### 1.3 Two-level lookahead (RFC-0024 §5)

These are in `palw_tir_dissect_v1.rs`, in the new section "RFC-0024 B".

- **Types:** `PalwTirLookaheadDisclosureV2` (`frontier[i]` = the claims of child i's pinned children, or child i alone when it is one tile) and `PalwTirChoicePathV2`.
- **`expand_lookahead_v2`** follows §5.3:
  - lengths and the bound are checked before any per-value work;
  - the shape and proven interval of every frontier claim are checked;
  - the level-1 claims are reconstructed by exact Σ/max (the RFC's omitted intermediate);
  - level 1 must fold to the disputed claim (`check_round_values`);
  - each level-2 group is checked against its child's projected phase and pinned cut.
- **`apply_lookahead_disclosure_v2` / `apply_choice_path_v2`** apply exactly the V1 transitions in sequence: round, choice, round, choice. B-01 equivalence therefore holds by construction, and the test also asserts it. The path length must be 1 plus "child is cut further".
- **Frontier bound:** `PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2: Option<usize> = None`. While it is unadopted, the lookahead is refused by name and the V1 one-level rounds remain the path (§5.5: "足りない場合は profile 未対応").

### 1.4 Multiproof (hash-only)

`PalwStepMultiproofV2` (`build`, `root`, `verify`, `leaf`, `encoded_bytes`) uses the step tree's own leaf domain, node hash and promote-odd shape (`step_merkle_leaf_v1` / `step_merkle_node_v1`). Shared paths are deduplicated, and a sibling is taken from the proof only when the walk cannot derive it.

Parser limits are checked before any hashing:
- leaf cap 65,536;
- ascending, distinct, in-range indices;
- proof length ≤ indices × depth;
- an unconsumed or short proof is refused.

`palw_step_single_openings_bytes_v2` gives the comparison count.

## 2. Tests (written, not run)

`kaspa-consensus-core` lib tests, `palw_tir_position_terminal_v2::tests`:
- `a_multiproof_roots_to_the_step_tree_for_every_shape`: n ∈ {1,2,3,5,7,8,9,16,17,33,100} and several index sets.
- `a_tampered_multiproof_is_refused`: bad leaf, short or long proof, moved index, unsorted, wrong count.
- `a_multiproof_carries_fewer_bytes_than_single_openings`: a deterministic byte count, not timed.
- `the_position_witness_budget_is_typed_missing_until_adopted`

Integration test `cargo test -p kaspa-consensus-core --test palw_tir_position_terminal_v2`:
- `a_middle_position_of_a_wide_tile_is_named_acquitted_honestly_and_convicted_when_it_lies`: the site's `tile_positions` is raised to 4, so tiles have ≥3 positions and a strict middle.
  - An honest middle acquits.
  - A Σ-preserving lie moved into the middle is convicted there.
  - The V2 signed-object path names the same narrowing.
  - The honest choice's digest cannot be replayed against the lying round.
- `a_position_move_bound_to_another_session_round_tile_or_claim_is_refused`: covers A-03-style tampers.
- `a_two_level_lookahead_and_one_choice_path_equal_the_old_two_rounds`:
  - B-01 equivalence;
  - B-03 (missing, duplicate, altered, extra group);
  - a wrong path length;
  - a path against another disclosure;
  - `None` and too-small bounds refused.
- `the_position_witness_budget_refuses_without_an_adopted_bound_and_never_convicts`

The existing `--test palw_tir_dissect_position` tests are untouched.

## 3. Byte accounting (**derived** from code constants and borsh framing, **not measured**)

Constants: Hash64 = 64 B, ML-DSA-87 signature = 4,627 B (`MLDSA87_SIGNATURE_LEN`), i128 = 16 B, Vec length prefix = 4 B, u64 = 8, u32 = 4, u16 = 2, enum tag / `Option` tag = 1. `w` = tile width (positions), `e_i` = demanded elements of reduction i, `n` = reductions, `k` = read-set leaves, `d` = step-tree depth, `p` = multiproof sibling count.

| object | formula | example |
|---|---|---|
| `PalwTirPositionDissectionV2` | 220 + w·(4 + Σᵢ(4 + 16·eᵢ)) | w=16, n=3, Σe=12: 220 + 16·(4+12+192) = **3,548 B** |
| `PalwTirPositionChoiceV2` | 2+64+4+64+1 = **135 B** | — |
| `PalwTirPositionCloseV2` fixed part | 2 + 64 + 1 + (4+4,627) + 135 + (4+4,627) + 1 (proof tag) + 1 (`Option`) = **9,466 B** + dissection + bottom carriage | above: 13,014 B **before** the bottom |
| `PalwStepMultiproofV2` | 20 + 72·k + 64·p | k=72, d=12 (test set), p=76 (computed by a Python port of the walk): **10,068 B** |
| k single `PalwStepOpeningV1` | k·(76 + 64·d) | k=72, d=12: **60,768 B** (the multiproof is 16.6% of this) |
| lookahead body (depth 2, arity a) | 6 + Σ_groups(4 + Σ_claims(4 + Σᵢ(4 + 16·eᵢ))) | upper bound a²·r·16 B on values: a=16, r=4,096 → **16 MiB** (RFC §5.5) |
| tag 203 object | + 64 (session) + 4 + 4,627 (sig) | — |

Observation for reviewers: the two ML-DSA-87 signatures alone are 9,262 B. The RFC's "約 12 KB" `B_position` estimate leaves about 2.7 KB for the round and the bottom, and the w=16 example round alone exceeds that. The budget is therefore left typed-missing, as required. It must be derived per profile and not taken from the estimate.

## 4. Fences needed (to arm centrally at testnet-12 DAA 16000)

- `palw_tir_bounded_door_v1`: owns tags 201–204, the extras mirror and the acceptance arms. Arming it also requires undecodable-payload tolerance (`validate_palw_lifecycle_kind_fences_v1`). It also requires its own G14 gate (`palw_g14_gates_v1.rs`: frame cost + public conviction + a pinned frame-cost record).
- TIR, gen and improvement position closes also need `palw_tir_v1`, `palw_gen_v1` and `palw_improvement_v1` respectively.
- Even when armed, position closes and lookaheads are **refused** until the user/ADR adopts `PALW_TIR_POSITION_WITNESS_BUDGET_V2` and `PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2`.

## 5. Design decisions (within RFC-0024 / ADR-0190/0191 scope)

1. **The whole-tile close stays admissible past the door.** The position round has no on-chain responder clock in state yet: 201 folds no state. If the whole-tile close were refused past the fence, a responder who never disclosed positions would leave the challenger unable to close. The backstop would then end the session challenger-side, and fraud would escape.
   - The position close is an additional, bounded path through the same verdict/fold.
   - Refusing the whole-tile close needs a phase/state slot for the position round and its deadline: a new state field, which needs review.
2. **One close path.** Tag 202 is applied as `CourtClosed` with its own dissection-bottom proof, so conviction, acquittal and the held-forfeit and attribution rules are not duplicated.
3. **Lookahead without new state.** 203 sets the phase's pending round to the reconstructed level 1. 204 re-carries the disclosure and re-checks it against `pending`, so the fold needs no stored frontier.
4. **The challenger signs the position** (choice digest bound to the exact round), as for V1 child choices.
5. **Amounts are left typed-missing** (witness budget, frontier bound): RFC-0024 §4.4 / §5.5 say "derive from profile; otherwise gate closed".

## 6. Not done or uncertain

- **Uncompiled.** The risk spots are:
  - the new borsh `PalwTirPositionCloseV2` nesting `PalwCourtVerdictProofV2`;
  - closure/`&V` generic passing in `check_tir_position_close_signatures_v2`;
  - the integration test's `Vec<Box<dyn Fn(&mut PalwTirPositionDissectionV2)>>`;
  - `generated.rs` / `wire_generated.rs` (`native_struct!` for `PalwTransitionExtrasV1` gained `tir_bounded_door_active`, and the enum gained 4 variants), plus the docs/maintenance manifests. These need the central regeneration.
- **Fixture override.** The middle test raises `site.tile_positions` to 4 on the fixture site. If the evidence builder reads `h_tile` from the class layout rather than the phase, the honest round may not fold. In that case, use a fixture with `h_tile ≥ 3`.
- **Shared bottom not adapted.** `palw_court_shared_tir_bottom_v1.rs` stores a signed row (`SignedSharedTirBottomV1`). A position variant changes a stored row layout and needs a v2 row; it is not done.
- **No position-round responder clock** (see §5.1). There is no `PositionOperandResolverV2` writer-row resolver (§4.4 A-05), so the witness carries the existing whole-leaf carriage restricted to `[p,p+1)`.
- **Lookahead ends at tiles.** It does not yet bundle the tile→position step into its last level (§4.2 "B の末尾 bundle に含めてよい").
- **Court capacity charge.** `palw_court_capacity_fold_v1::charge_court_op_v1` charges the new kinds by its default rule. It was not reviewed for the 202 re-entry: the outer object is charged; the inner `CourtClosed` passes through `apply_object_inner`, not the charge.
- The two lifecycle-table tests `every_landed_kind_is_pinned_to_its_wire_form` and the A2U sweeps should be run first.
