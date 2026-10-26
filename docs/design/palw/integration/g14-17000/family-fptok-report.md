# Family lane FPTOK (`FpTokenizer`) — report 2026-10-26

Branch `claude/g14-fam-fptok` from `d8aded6de` (`claude/g14-panel1-17000`). Nothing pushed, no preset or activation change
(every shipped preset keeps `palw_fp_tokenizer_match = None`). No arming record was added.

## 1. Stage-by-stage state table

| stage | state | where |
|---|---|---|
| job ↔ tokenizer binding (which tokenizer a claim is judged under) | implemented, dormant fence `palw_fp_tokenizer_match` (RFC-0001 §2.9): past it a job must name its class's listed tokenizer; a class listing none (dense A16 / legacy) is SILENT | `palw_fp_tokenizer_v1.rs`, extraction walk `palw_fp_objects_v3.rs` |
| **binding as part of the G14 route** | **was missing — closed here**: `route_armed(FpTokenizer)` did not require the binding, so the family could be counted armed while a job named a tokenizer other than the one the court's registered raw material describes | `palw_g14_gates_v1.rs` |
| tokenizer material as public authenticated material | class registry rows carry `raw_tokenizer_bytes` + `tokenizer_key_events` (+ table root, rendering rule); raw JSON / key proofs / rendering are replayed against them (hash/Merkle identity only) | `palw_tokenizer_table_stream_v1.rs`, `palw_tokenizer_json_stream_v1.rs`, `palw_tokenizer_canonical_replay_v1.rs` |
| acceptance (atomic, inclusion prerequisites, capital row) | F02 shared parts; tokenizer family has no own admission row beyond the staged seed | lane F02 / FPCP |
| duty / reservation (one reserver, paid, owned court) | implemented: `PalwOwnedTokenizerCourtV1` (literal ordinary reserver, rent, deposit, move and carrier budget) | `palw_tokenizer_owned_court_v1.rs` |
| supply → honest Final / reward | staged court + LG14-A filer reservation holds Final; reward returns only via `route_armed` (G-01 (b)) | `palw_staged_court_fold_v1.rs`, LG14-A |
| lie: wrong detokenize / tokenize | deterministic recompute from registered raw material (EOF-checked), staged raw-tokenizer family | `palw_staged_court_fold_v1.rs` (raw_tokenizer_bytes branches), `palw_court_shared_tokenizer_family_v1.rs` |
| lie: other tokenizer used | now: refused at extraction past `palw_fp_tokenizer_match` (skip by name), and the family is not routed without it | this lane |
| outsider conviction → slash | staged court verdict + LG14-A filer; native fixture tests exist (2026-10-22 evidence: actual-original/EOF reconstruction, fresh recheck); **no funded live-node E2E** | evidence `tokenizer-and-graph-preflight.md`, `parameter-and-tokenizer-producer.md` |
| silence → default; unavailable material | material unavailable is a DA default, not a computation fault (LG14-B / staged held units); not re-verified in this lane | LG14-B, FPCP |
| reorg / restart / pruned IBD | owned court checkpoint is borsh state; no tokenizer-specific V7 run | open |

## 2. Implementation (this lane)

- `PalwG14RouteMirrorV1.tokenizer_match_from` (resolved `palw_fp_tokenizer_match_fence()`), filled by
  `palw_g14_route_required_v1_mirror` (central, `palw_g14_gates_v1.rs`). The mirror is a derived param, not serialized or hashed.
- `route_armed(FpTokenizer)` = staged court ∧ LG14-A ∧ tokenizer match; FpConstraint / FpPrefix unchanged.
- `palw_g14_route_family_fences_v1(FpTokenizer)` adds `palw_fp_tokenizer_match` (an existing fence, already visited by
  `for_each_fence` / fork-id; t12 identity unchanged since it stays `None`).
- Tests: `g14_close_gates` T-N10 now asserts every FP job stays refused while the tokenizer is unbound; `g14_close_legacy`
  `fp_recoverable_families_have_no_remaining_lg14a_dependency` arms the binding in its full FP ruleset.

No new value was needed (no `palw_g14_values_v1` change).

## 3. Tests

`buildslot.sh … cargo test -p kaspa-consensus-core --no-fail-fast --test g14_close_gates --test g14_close_legacy [--test g14_court_capacity]` (exit 0 from the pipe; cargo reported failures):

- `g14_close_gates`: **12 passed, 0 failed** (incl. T-N8, T-N10 and the g14-genesis family test, updated for the binding).
- `g14_close_legacy`: 1 passed, 8 failed. Seven fail at the shared fixture `:94` (`validates with its records`) or the
  t12 identity pin `:841`; `g14_court_capacity` shows the same pattern (13 failed at `:120` / identity `:1379`). These paths do not
  touch this diff (the mirror is not hashed; no params changed), so they look pre-existing on `d8aded6de`, but **this was not
  checked against the base build**.
- `fp_recoverable_families_have_no_remaining_lg14a_dependency` fails at `:237` (the "door past the horizon" `expect_err`
  message check). **Not resolved**: it may be caused by arming `palw_fp_tokenizer_match` in that ruleset (its validation
  prerequisites) or be the same pre-existing fixture drift. Needs one base-vs-branch run.

## 4. Remaining

1. Classes that list no tokenizer (`PalwFpTokenizerRuleV1::Unlisted`, dense A16 legacy rows) have no registered raw tokenizer
   material, so the staged raw-tokenizer court cannot judge them; `route_armed` is class-agnostic and would count them armed.
   Needs a class-aware check at the FP claim gate (`palw_g14_route_fp_refusal_v1`) — see request 1.
2. Funded live-node E2E of a tokenizer lie (V2/V3), V5 measured cost of raw JSON replay at the largest genesis tokenizer
   (Qwen3.x / gpt-oss vocab), V7 reorg/restart mid-court: not run.
3. `palw_fp_tokenizer_match` is not a `PalwG14GatedFenceV1` and has no arm-readiness record type of its own; its prerequisites
   (`palw_tir_v1`, `palw_fp_decode_rules`) are validated by `validate_palw_fp_tokenizer_match_v1`.

## 5. Requests to the coordinator

1. Classifier/SPINE: refuse (G14_ROUTE_ABSENT) an FP claim whose class's tokenizer rule is `Unlisted` once option (b) governs,
   since no public raw tokenizer exists to recompute against (not a computation fault of the producer).
2. Include `palw_fp_tokenizer_match` in the 16,000/17,000 fence list at or below `palw_staged_public_court_v1`, and decide whether
   it gets a `PalwG14GatedFenceV1` record (proposed requirements: the extraction-skip fold test, node E2E of a mismatched job,
   fence-crossing drill; no measured value).
3. FPCP: confirm the staged raw-tokenizer verdict reads the tokenizer from the class row (not from the job), which is what makes
   the binding sufficient.

## 6. Follow-up (coordinator request, 2026-10-26)

**(1) Base vs branch.** Both were run with the same command: `cargo test -p kaspa-consensus-core --no-fail-fast --test g14_close_legacy --test g14_court_capacity`. The base run used `d8aded6de`'s three files and its own `CARGO_TARGET_DIR=target-base`.

| target | base `d8aded6de` | branch | classification |
|---|---|---|---|
| g14_close_legacy | 1 pass / 8 fail | 1 pass / 8 fail (same tests) | all **base pre-existing** |
| g14_court_capacity | 2 pass / 13 fail / 1 ignored | same | all **base pre-existing** |
| fp_recoverable_families_have_no_remaining_lg14a_dependency (:237) | FAIL | FAIL | **base pre-existing** |
| g14_close_gates | not run on base | 12 / 12 pass | — |

Every failure has the same cause. Validation of the armed ruleset is refused with `Invalid("OneOutsiderCapacityIncomplete: RFC-0026 F03 … incomplete; ServiceOpportunityIncomplete: F02 … not adopted")` (from the F02/F03 slice lanes). The t12 identity pins (`5ee7fd8e` / `5de80e64` / `1678e073`) are also stale on base. None of these failures comes from this diff.

**(2)** The :237 failure is not caused by this diff. The FpTokenizer fence list is now ordered binding → LG14-A → staged court. No params validator ties the binding below the staged court, because Constraint and Prefix legitimately use the staged court without the binding. `route_armed` counts the family only from the latest of the three fences. Placing the binding at or below the court is therefore a fence-list (release) choice. Changing this order is a reorder of a string array only, and was not rebuilt.

**(3) Not implemented.** `palw_g14_route_fp_refusal_v1` receives no class row: its arguments are params, DAA, commitment and framed. Only its caller `palw_state_v2/objects.rs:~3669` holds `builder.state` and can read the class's listing (`tir_class_v1` / `PalwGenClassRecordV1::tokenizer_id`, and the class row's `raw_tokenizer_bytes`). Proposed connection: add a `class_has_tokenizer_material: bool` argument (`raw_tokenizer_bytes.is_some()` on the class's registered semantics) and drop `FpTokenizer` from `route_armed` when it is false, which returns `G14_ROUTE_ABSENT`. Tests to add: a listed class is admitted and an unlisted class is refused, both with every route armed. This changes the shared fold caller, so it is left to the owner of `objects.rs` (SPINE / F02).

## 7. Proposal (3) implemented (coordinator request, 2026-10-26)

- `palw_g14_route_fp_refusal_v1(.., class_has_tokenizer_material: bool)`. When the flag is false, `FpTokenizer` counts as missing, and only when it would otherwise be armed. The function returns `None` below the fence before it reads the flag. When the tokenizer route is unarmed, the family is missing whether the flag is true or false.
- Caller `palw_state_v2/objects.rs` (FP claim gate): the flag is set when `class_semantics[class]` has both `raw_tokenizer_bytes` and `tokenizer_key_events`.
- Tests in `legacy_fp_selection_and_constraint_jobs_are_refused_until_their_route_is_armed`, each run for V4, V6 and V11:
  - With every route armed, a registered class is admitted and an unregistered class is refused with `G14_ROUTE_ABSENT`.
  - Below the fence the answer is `None` and identical for both flag values.
  - With the tokenizer route unarmed, or with no route armed, the answer is identical for both flag values.
- Run: `cargo test -p kaspa-consensus-core --test g14_close_gates` gave **12 passed / 0 failed**.
- Not tested: a fold or node test of an unregistered class's FP claim through `objects.rs` (only the function itself is tested).

## 8. Arming push (coordinator, 2026-10-27; target now DAA 17,000, drill skipped)

Merged `claude/g14-panel1-17000` (tip `56c5ca067`) into this branch.

**Run and passed** (debug, buildslot):
- `cargo test -p kaspa-consensus-core --no-fail-fast --test g14_close_gates --test g14_fptok_candidate` → g14_close_gates 12/12. This includes the new fold-level check in T-N8: with every route armed, an FP claim on the floor class (no class semantics, so no raw tokenizer material) is refused through `objects.rs` with `G14_ROUTE_ABSENT`, and `FpTokenizer` is named among the missing routes.
- `cargo test -p kaspa-consensus-core --test g14_fptok_candidate` → 2/2:
  - `fptok_candidate_covers_the_family_in_activation_order`: the candidate is the kernel slice followed by tokenizer_match → LG14-A → outsider lane → staged court, then route-required. `route_armed(FpTokenizer)` is false at 16,999 and true at 17,000.
  - `fptok_candidate_is_refused_without_records_and_adds_no_refusal_beyond_the_slice`: without records the candidate is refused (first refusal: the kernel-route record). With test-only records for every gated fence it fails with exactly the same error as the kernel slice alone: `palw_settlement_once_v1 stays dormant: open blocker — RFC08-2 (unsourced terms)`. **The FpTokenizer fences add no refusal of their own.**
- F02/F03 check against the pinned estimates: the staged court's completeness gate passes with `PALW_COURT_SERVICE_PROFILE_V1` = the approved estimate, `palw_court_ingress_schedule_v1()` = `PALW_G14_COURT_INGRESS_ESTIMATE_V1`, and the outsider lane at or below the court. The candidate therefore needs `palw_court_outsider_lane_v1` at or below `palw_staged_public_court_v1`.
- The real arming lists are unchanged (the test builds its own candidate).

**Waiver items (not run; the user asked to avoid long builds and tests):**
1. Native E2E of a full-collusion tokenizer lie convicted by one outsider, with its honest control. No in-process harness exists for the staged raw-tokenizer court. Past `palw_fp_tokenizer_match`, a job naming a different tokenizer is dropped at extraction (it is never admitted, so nothing needs convicting). A wrong detokenize under the correct tokenizer has core-level evidence only (2026-10-22 `tokenizer-and-graph-preflight.md`).
2. Fold-level positive control (a class WITH raw tokenizer material admitted through `objects.rs`). Class semantics cannot be set from an integration test, and a framed FP admission needs the funded-original path. The positive case is covered only at function level.
3. Restart / reorg / pruned IBD for the tokenizer court and route gate: not run. The route gate is stateless (it reads fence mirrors and class semantics, which are already rooted and carried through pruning snapshots).
4. Measurement at maximum vocabulary: **not run**. An ignored harness was added, `palw_tokenizer_json_stream_v1::tests::fptok_measure_tokenizer_replay_cost`. It times the verifier and producer separately on synthetic vocabularies of 1k/4k/16k entries and extrapolates per event. Sizes counted with Python, no timing:

| tokenizer.json | bytes | vocab | merges | key events (≈ object keys + vocab ids + added ids) |
|---|---|---|---|---|
| Qwen3.6-35B-A3B (largest genesis vocab on disk) | 19,989,343 | 248,044 | 247,587 | ≈ 496,391 |
| Qwen3.5-2B | 12,807,982 | 248,044 | 247,587 | ≈ 496,335 |
| Qwen2.5-1.5B | 7,031,645 | 151,643 | 151,387 | ≈ 303,500 |

   Each key event carries a 512-sibling proof, so a verifier does ≈ 512 hashes per event, ≈ 2.5 × 10^8 hashes for Qwen3.6. The local producer's sparse tree holds ≈ 513 nodes per event (≈ 2.5 × 10^8 entries), so producing the material is memory-heavy; this needs measuring before relying on the court's tokenizer budget. Gemma4 (262k vocab) is not on disk. To run: `cargo test --release -p kaspa-consensus-core --lib fptok_measure_tokenizer_replay_cost -- --ignored --nocapture` on an idle host with `uptime` recorded, evidence under `docs/evidence/palw/g14-17000/measurements/`.
5. `g14_close_legacy` / `g14_court_capacity`: not rerun after the merge (their failures before the merge were base pre-existing, §6).

**Left before arming FpTokenizer at 17,000:**
- Add tokenizer_match, LG14-A, the outsider lane and the staged court to the real 17,000 list, in the candidate's order, before route-required.
- Pin their records.
- Clear the slice's own `palw_settlement_once_v1` blocker (RFC08-2).
- Get a waiver or a run for items 1–4.
