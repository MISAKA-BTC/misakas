# G14 parallel lane r6-p2-shared: RFC-0024 P2 remainder (2026-10-22)

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/r6-p2-shared`, base `3c6e4c09d`. Continues
`r5-p2-terminal.md` §6 ("not done") and `court-archive-terminal.md` Task 2. Design authority: RFC-0024 §4 (A), §4.2, §4.4,
§5 (B), §7, §11 (T15); ADR-0190 (door fence `palw_tir_bounded_door_v1`), ADR-0191 (2). Hash and integer folds only; no
proof of computation (PRINCIPLES §8).

## 1. What was implemented

### 1.1 Position round ON CHAIN with its per-turn deadline; whole-tile close retired for NEW claims (item 4)

No phase field and no borsh change. Two phase encodings that no older transition writes carry the position round
(`consensus/core/src/palw_tir_dissect_v1.rs`, section "RFC-0024 A — the position round ON CHAIN"):

- **Owed**: `turn == AwaitDisclosure` with `tile_count == 1`. Every older transition sets `Terminal` at one tile.
  - `owe_position_round_v2` puts a terminal tile wider than one position into this state. The deadline is the one the move that made it terminal already set (`accepted + turn_deadline_daa`, capped to the session by `cap_tir_phase_deadline_v1`): the same derivation as every dissection round.
  - The existing sweep (`deadlines.rs`, `court_session_turn_and_rung_deadline_v2`, `court_next_deadline_v2`) reads it as an ordinary `AwaitDisclosure`. Silence past the deadline is the **responder's** no-show (`declare_no_show` → `Responder`), so fraud cannot reach the challenger-side backstop. No sweep code changed.
- **Disclosed**: `turn == Terminal`, `tile_count == 1`, `pending` = one claim per position.
  - `install_position_round_v2` / `apply_position_round_v2` check the round with the r5 position-round rules (shape, intervals, exact Σ/max fold to the tile claim), set `pending` and set the challenger's deadline to `accepted + w_round`.
  - At `Terminal` nobody is rung-charged; the backstop rule is unchanged.
- **Guards:**
  - `check_round_header` and `expand_lookahead_v2` refuse a tile round or lookahead on one tile.
  - `whole_tile_terminal_range_v2` is `None` once a position round is on chain. The whole-tile bottoms now use it: `palw_tir_court_v1::check_tir_dissect_bottom_v1`, and `palw_gen_court_v1` line ~1164 (gen and eval through it).
  - While the round is owed, `terminal_range()` is already `None`, so the whole-tile close is refused there too.
- **Position close is bound to the round on chain:** `palw_tir_position_terminal_v2::check_tir_position_dissection_v2` uses `position_narrowing_v2` (an owed round narrows as the terminal it will be). Once a round is disclosed, the close's round must equal the round on chain. A close against an owed phase is refused by `check_tir_dissect_position_bottom_v1`'s tile-equality check, so tag 201 must come first.
- **Fold** (`palw_state_v2/objects.rs`):
  - `owe_tir_position_round_v2` (in `palw_state_v2.rs`) runs before every `cap_tir_phase_deadline_v1` (7 sites: Tir/Gen/Eval root opens, round, child choice, lookahead disclose, lookahead choose; it is a no-op unless the phase is a fresh one-tile terminal).
  - **Tag 201** now installs the round where it is owed (`apply_position_round_v2`, then `write_tir_dissection` and `write_court`, which re-indexes the clock). Otherwise it stays stateless, as in r5.
  - **Tag 203** refuses a version-3 disclosure for a claim whose position terminal does not apply.
- **T15 selector:** `palw_state_v2::palw_tir_position_terminal_applies_v2` (and `_with_v2`). It is true only when all of these hold:
  - the door is active at this block;
  - the claim's own `accepted_daa >= fence DAA` (older claims keep the whole-tile close);
  - `PALW_TIR_POSITION_WITNESS_BUDGET_V2.is_some()`. Without an adopted bound every position close is refused, so retiring the whole-tile close would let fraud escape. The const is `None`, so nothing is retired until the bound is adopted.
- **New extras field** `PalwTransitionExtrasV1::tir_bounded_door_from_daa: Option<u64>`:
  - set in `consensus/src/.../palw_accept.rs` from `palw_tir_bounded_door_v1` (`None` when unarmed or `never`);
  - added to the two explicit constructions in `palw_state_v2/tests.rs`.

### 1.2 Lookahead whose last level ends on one history position (item 3)

- `PALW_TIR_LOOKAHEAD_POSITION_VERSION_V2 = 3`: the same `PalwTirLookaheadDisclosureV2` body, so there is no layout change.
- A level-1 child that is ONE tile wider than one position carries that tile's position round (one claim per real position, ragged end included). The tile claim is reconstructed by the exact fold, then interval- and fold-checked as level 1 (§5.3 rule 2).
- `PalwTirLookaheadExpansionV2` gains `positions`.
- `apply_choice_path_v2`: after the tile choice, `install_position_round_v2` puts the disclosed positions on chain. B-01 holds by construction: one bundle + path ≡ tile round + choice + tag-201 position round.
- The path stays one index for that child. The position itself is named by the position close under the challenger's signature (§4.2: "論理的な検証段は残すが、独立した response 往復を必ず追加する必要はない"). This saves the responder's 201 round trip. It does not save the close.
- **Depth limit kept at 2:** a level-2 child that is one tile still owes a separate 201.

### 1.3 Shared court: single-position bottom, v2 stored row (item 1)

`palw_court_shared_tir_bottom_v1.rs`:
- `SignedSharedTirPositionBottomV2 { control (version 2), position: u8 }`:
  - own intent domain `…/shared-tir-bottom-intent/v2`, which binds `position`;
  - own slot `shared_position_bottom_id_v2(root_id, path, position)`, so it never collides with the whole-tile v1 slot.
- `check_shared_tir_position_bottom_v2` → `check_statement(.., Some(position))` → resolver (below) → `choose_position_v1` → `check_tir_dissect_position_bottom_v1`, which evaluates over `[p,p+1)` against root totals.
- Outcome digest `…/shared-tir-bottom-result/v2` binds the position.
- **`SharedTirBottomRowV1`:**
  - new field `position: Option<u8>`, with a manual `BorshSerialize`;
  - **row version 1 is byte-identical** (no position byte). Row version 2 = control v2 + one trailing position byte;
  - the decoder accepts 1|2; v1 rows decode unchanged;
  - `check` requires (1,1,None) or (2,2,Some);
  - `id()` picks the slot.
- **Fold** (`palw_court_shared_tir_fold_v1.rs`): `position_bottom` is refused below `extras.tir_bounded_door_active`. `bottom` and `position_bottom` share `bottom_inner`. The resource request counts two extra transient bottom copies for the position form.
- **Staged-row writer** (`palw_staged_court_fold_v1.rs::write_staged_row_v1`) re-checks a v2 row through the position check and refuses it below the door.
- **Dispatcher** (`palw_court_service_lifecycle_fold_v1.rs`): `SharedCourtOpV1::PositionBottom`. A newly stored `NativeFault` takes the same conviction funnel (`apply_shared_bottom_conviction_v1`).

### 1.4 Writer-row resolver (item 2)

- **Shared rounds** (`palw_court_shared_tir_round_v1.rs`):
  - `SHARED_TIR_POSITION_ROUND_VERSION_V2 = 2`. The position round is the original signer's shared round at the terminal tile's OWN path, a slot no v1 round can fill.
  - It is checked as the dissection's position round: `position_narrowing_v1` + `check_position_round_v1`.
  - A v2 round can never be an ancestor (`prepare_shared_tir_round_v1`).
  - Row layout is unchanged (row version 1, control version 2). The fold and the staged writer refuse it below the door.
- **`resolve_shared_position_writer_row_v2(state, claim, leaf, root_id, path, at_daa)`** resolves the row from rooted state:
  - finds `Key::CourtSharedRound(round_id(root, path))`;
  - requires control version 2 and the same claim, leaf, root and path;
  - requires that the row is not later than the bottom;
  - re-runs `check_context` (shape, signature, numerical context).

  Nothing the filer carries can substitute it.
- **Operand side (RFC-0024 §4.4 "PositionOperandResolverV2"):** not a new code path. The position bottom's carriage is already forced to be exactly `[p,p+1)`'s canonical read set (`check_canonical(&source.used)`), authenticated under the claim's step root and preceding the disputed leaf (`authenticate_operands`). The §4.4 equivalence, coverage and size proof (writer row vs HistTile copy) is **not** done. That is why the witness budget stays typed-missing.

## 2. Tests (written, not run)

Integration target `cargo test -p kaspa-consensus-core --test palw_tir_position_round_onchain_v2` (new file `consensus/core/tests/palw_tir_position_round_onchain_v2.rs`):
- `the_position_terminal_applies_only_past_the_fence_to_claims_admitted_past_it_and_with_a_budget` (T15 table)
- `a_new_claims_wide_terminal_owes_the_position_round_and_its_silence_is_the_responders`:
  - owed state;
  - no tile round, lookahead or whole-tile close;
  - responder no-show past the deadline;
  - install sets the challenger's deadline `50+W`;
  - once only;
  - whole-tile retired;
  - honest position close acquits;
  - another round is refused.
- `a_claim_admitted_before_the_fence_keeps_the_whole_tile_close`
- `a_lookahead_ending_on_positions_equals_the_tile_choice_plus_the_position_round`:
  - B-01 equivalence;
  - B-03 (missing, duplicate, altered position);
  - v2/v3 confusion refused;
  - a 2-index path is refused;
  - `None` bound refused.

`kaspa-consensus-core` lib test, extended in place: `palw_state_v2::palw_tir_public_fold_v1::tests::shared_history_root_binds_public_original_policy_and_literal_producer_signature`. It covers:
- v1 row bytes are unchanged, and v2 = v1 + 1 byte, round-trips;
- distinct slots per position;
- mismatched version/position refuses to encode or check;
- the v2 signing message binds the position, and a v1 control is refused;
- below the door, `fold::position_bottom` and a v2 `fold::round` are refused with state root and journal unchanged;
- the resolver finds nothing without a position round.

Not covered by a test (needs a door-armed shared fixture): a stored v2 position bottom convicting or acquitting end to end on the shared path.

## 3. Byte accounting (**derived** from borsh framing and constants, **not measured**)

Notation as in `r5-p2-terminal.md` §3. `w` = tile width, `C = 4 + Σᵢ(4 + 16·eᵢ)` = bytes of one claim. Example: n=3, Σe=12, so C = 208.

| item | formula | example |
|---|---|---|
| shared bottom row v2 vs v1 | +1 B (trailing `position`) | +1 B |
| shared position-bottom signed intent | the hashed tuple adds 1 B (`position`); the carried control is unchanged | — |
| shared position round (writer row) | same row framing as a tile round; body 2 + 4 + w·C (a tile round has `arity` children) | w=16: 3,334 B body |
| on-chain phase while disclosed | `pending` grows from 4 B to 4 + w·C, held until the session ends (layout unchanged) | w=16: 3,332 B |
| tag 201 (unchanged object) | r5 §3: 220 + w·C + sig 4,627 + framing | w=16: ≈ 8.2 KB |
| lookahead v3 vs v2, per single-tile child | +(w−1)·C | w=16: +3,120 B per child |
| lookahead v3, all `a` children single tiles | 6 + a·(4 + w·C) | a=16, w=16: 53,318 B (vs v2 3,398 B), saving one 201 (≈ 8.2 KB) and one response window |

Per §5.5 the v3 bundle is only a time optimisation when `saved_wait > added_cost`. The frontier bound `PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2` stays typed-missing (`None`, so the bundle is refused). The w=16 v3 body (≈ 53 KB) shows that the bound must be derived per profile, not assumed.

## 4. Fences needed (arm centrally at testnet-12 DAA 16000)

- `palw_tir_bounded_door_v1` owns all of it:
  - the extras `tir_bounded_door_active` and the new `tir_bounded_door_from_daa`;
  - shared v2 rounds and bottoms;
  - the owed position round;
  - lookahead v3.

  It is still gated by its G14 gate (frame cost + public conviction + pinned record).
- The shared path additionally needs the shared-lifecycle fences (`palw_tir_public_conviction_v1` + `palw_court_outsider_lane_v1`) for the dispatcher.
- **Retiring the whole-tile close needs `PALW_TIR_POSITION_WITNESS_BUDGET_V2` adopted** (currently `None`, so not retired). The lookahead also needs `PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2`. Both are user/ADR decisions (RFC-0024 §4.4/§5.5); they are not set here.

## 5. Design decisions (within RFC-0024 / ADR-0190/0191)

1. **No new state field.** "Owed" and "disclosed" are encodings the older transitions never produce (`AwaitDisclosure`@1 tile; `Terminal`+`pending`), so no borsh layout, wire tag or below-fence behaviour changes. The deadline reuses `turn_deadline_daa` + `cap_tir_phase_deadline_v1` (the "same derivation" the task asks for). Silence is charged by the existing sweep, with no new sweep code.
2. **T15 by the claim's `accepted_daa`**, the established pattern (`claim.accepted_daa >= fence`, e.g. `capacity_*_at`).
3. **The budget gates the retirement** (`applies_v2` requires `Some`). Otherwise an armed door with an unadopted budget would leave new claims with no admissible close, which is exactly the backstop escape the task forbids.
4. **The challenger names the position in the close, not in the lookahead path** (no stored chosen position; §4.2 allows the step to stay logical). The close's round must equal the round on chain.
5. **Shared:** the writer row is the original signer's v2 shared round at the terminal path, which reuses the shared round row, signature and context checks. The position bottom has its own slot and intent domain. The v1 whole-tile shared bottom stays admissible: shared bottoms are local evidence, not the session close.

## 6. Not done / uncertain

- **Uncompiled.** Risk spots:
  - `SharedTirBottomRowV1`'s manual `BorshSerialize`/new field versus `generated.rs`/`wire_generated.rs` (`native_struct!`/`wire_struct!` list fields: **central regeneration required**, also for the extras field);
  - `Ok(&**row)` in the resolver;
  - the 10-tuple in the v2 intent digest (borsh tuple arity);
  - the borrow shadowing in the tag-201 fold arm.
- **No wire carriage for the shared position bottom / position round.** The tag-178 numerical transport decodes only `version == 1` controls and has no position-bottom kind. Its Round ceiling uses `arity` children, but a position round has `w`. The dispatcher variant `SharedCourtOpV1::PositionBottom` is the entry such a transport kind must call. Adding a `NumericalKindV1` variant plus the native-heap generated entries is a central step.
- The dispatcher's position-bottom conviction does not capture court-end archive evidence (no manifest/parts); it records as before r5-f02 (`Uncaptured`).
- §4.4 operand resolver equivalence and size proof: not done, so the budget stays `None`.
- **Responder tooling** (kaspad/builders) does not yet build tag 201 when it sees `AwaitDisclosure` on one tile. Until it does, an armed door with an adopted budget would convict honest responders by no-show. Ship the builder with the budget.
- The court capacity charge for tag 201's new state write was not reviewed (same note as r5 for 202).
- An end-to-end shared fixture with the door armed (a v2 round stored, then a v2 bottom convicting or acquitting) is not written.
