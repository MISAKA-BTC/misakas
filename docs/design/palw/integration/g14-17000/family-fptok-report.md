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
