# Lane material-intake — RFC-0032 A non-exclusive terminal material ingress (U13)

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/material-intake`, base `8b036ed38`.

Scope: RFC-0032 A (user-approved isolated implementation, 2026-10-21) / ADR-0199 — authenticated terminal material submitted by any eligible party in multi-block parts, rooted, retained to the claim's material horizon, with no first-come exclusive unfinished body; existing builders read it through an adapter. Nothing here is adopted, wired to a wire tag, or armed.

## What was implemented

| file | item |
| --- | --- |
| `consensus/core/src/palw_court_shared_material_intake_v1.rs` (new, `pub mod` in `lib.rs`) | `MaterialTargetV1` {Parameter{leaf}, PromptFlat, PromptTile{tile}, Unit{ordinal}, FullIds}; `SignedMaterialIntakeV1` + `MaterialActionV1` {Begin, Part, Finish} (ML-DSA-87, context `misaka-palw/material-intake/v1/mldsa87`); rows `MaterialDraftV1`, `MaterialPartV1` (bounded deserialize, 16 KiB parts), `MaterialRowV1` (canonical raw bytes, re-authenticated on load), `MaterialLedgerV1` (per claim × submitter reservation); `material_intake_schedule_v1` (typed-missing `MaterialIntakeMissingV1::Schedule` in every non-test build; `#[cfg(test)] set_material_intake_test_schedule_v1`); native requirement derivation `required_parameter_leaves_v1` / `target_bound_v1` / `claim_budget_v1`; authentication `decode_material_v1`; adapter `RootedMaterialV1::load_v1` + `store_v1` |
| `consensus/core/src/palw_court_shared_material_intake_fold_v1.rs` (new, `#[path]` mod in `palw_state_v2.rs`) | `material_intake` (atomic checkpoint/restore), `release_draft`, `expire_material_drafts`, write guard `TransitionBuilder::check_material_intake_write_v1` |
| `palw_staged_court_state_v1.rs` | appended keys `CourtMaterialDraft(claim,draft)`, `CourtMaterialPart(draft,offset)`, `CourtMaterial(claim,target)`, `CourtMaterialLedger(claim,submitter)` (key tags 26–29) and rows (row tags 32–35); `active_row*` / `check` arms. No existing tag changed. |
| `palw_staged_court_fold_v1.rs` | `write_staged_row_v1` calls the guard; `index_rows` due = draft/part `expires_daa`, material/ledger `material_until_daa`; `sweep` releases an expired draft (parts + reservation) and removes parts/material/ledger at their horizon |
| `palw_court_service_state_v2.rs` | `court_service_holds_material_v1`: canonical material and ledger are held like other shared original data; drafts/parts are not |
| `palw_state_v2/chain_state.rs` | `staged_court_range_v1` (ordered range read); load consistency re-checks draft and canonical material context |

Mechanism:

- **Eligibility**: any Active bond at/above the existing floor with a valid signature. No Ready, owner slot, panel seat or first-filer status is read or granted.
- **Non-exclusive**: the draft key binds the literal submitter (`material_draft_id_v1(claim,target,submitter,size,hash)`), so partial bodies of different submitters coexist. The first complete, checksum-matching, authenticated body writes `CourtMaterial(claim,target)`; afterwards further parts for that target are refused, a later Begin returns `AlreadyComplete` with no write, and a losing Finish releases its draft. Incomplete drafts expire at `min(begun + TTL, hard, material_until)` and are released by the sweep (or `expire_material_drafts`).
- **Authentication** (data identity only, existing commitments): parameter → `SharedTirContextV1::decode_parameter_opening` (exact registered width, program/inventory/root) and membership in the original's input-cover union; Flat prompt → exact width, count, `prompt_token_ids_match_v1`; Merkle tile → width bound, coordinate, `verify_prompt_ids_opening_v1`; units → `decode_response_v1` + `authenticate_with_rows_v1` (trace units need the rooted shared ID row); FullIds → `decode_ids_body` then the existing `share_ids` (canonical store stays `CourtSharedIds`).
- **Budgets, never declared**: per-target bound = exact native width (`palw_tir_param_carriage_bytes_v1(PerLeafOpenings,[leaf]) - 4`, prompt form width, `request_body_bound_v1`); per (claim, submitter) budget = Σ request-set unit bounds + complete-ID bound + parameter-union carriage + prompt bound — the same input cover (`root_input_cover_native_v1`) the merged preclaim derivation `palw_tir_prepared_original_requirements_v1.rs` retains. Each submitter gets the whole requirement, so a colluder cannot exhaust an honest submitter's room; Begin reserves the declared size, release returns it.
- **Funding**: `MaterialIntakeScheduleV1 {sompi_per_byte, draft_ttl_daa, cover}` — production typed-missing ⇒ every intake refused. With a test schedule the ledger records `charged_sompi`; **no debit is made** (the payer is part of the unadopted schedule).
- **Restart/reorg**: everything is staged rows through the ordinary writer, so block deltas (`apply_delta_entry`), carriage round-trip and reload re-authentication apply. Canonical material and parts are immutable under the guard.
- **Adapter**: `RootedMaterialV1::load_v1` reads canonical intake rows + existing rooted `CourtSharedQuery` bodies + `CourtSharedIds`, re-authenticates all, and `store_v1` hands them to the existing `SharedTirBuilderStoreV1::new_with_retained_parameters` (same borrowed views; no fallback).

## Tests (written, not run)

Crate `kaspa-consensus-core` (lib tests):

- `palw_state_v2::palw_tir_public_fold_v1::conviction_e2e_tests::material_intake_partial_bodies_coexist_first_complete_wins_and_reorg_reverts` — two submitters' drafts coexist; first Finish wins; loser refused parts / released; Begin after completion is a no-op; immutability; reorg revert/reapply roots; carriage reload; corrupted persisted bytes fail re-authentication.
- `…::material_intake_rooted_store_builds_identical_evidence_to_borrowed_path` — all step units, the cone's parameter openings and the Flat prompt submitted through intake; `RootedMaterialV1` store builds byte-identical `build_tir_cone_refutation_v1` output to the borrowed store; empty rooted store refuses.
- `…::material_intake_refuses_corrupt_parts_overbudget_and_dormant_fence` — dormant fence, missing schedule, forged signature, over-width declaration, non-input coordinate, corrupt part under honest checksum, self-consistent corrupt body, changed-bytes retry, per-claim budget refusal.
- `…::material_intake_incomplete_drafts_expire_and_release_reservation` — TTL, sweep release of draft/parts/reservation, explicit expiry.
- `palw_court_shared_material_intake_v1::tests::{material_schedule_is_typed_missing_until_injected, material_ids_bind_claim_target_and_literal_submitter, material_part_wire_is_bounded, prompt_tile_bound_covers_ragged_tile}`.

Run: `cargo test -p kaspa-consensus-core material_intake`.

## Fence

The RFC names no fence. The intake is gated by the existing dormant **`palw_staged_tir_route_v1`** (`extras.staged_tir_route_active`, G-10, the fence the shared TIR court lives under) **and** by the typed-missing storage schedule **and** by an installed `CourtSharedContext` (no production installer exists). Arming `palw_staged_tir_route_v1` alone does NOT activate intake: the schedule must also be adopted. No new params fence was added (avoids touching every preset literal/identity list in parallel lanes); if central review prefers a dedicated fence, add `palw_material_intake_v1` and replace the `staged_tir_route_active` check in `material_intake_inner`.

## Central step required before it compiles

- `palw_native_resource_v1/generated.rs` `native_enum!` lists for `PalwStagedCourtKeyV1` / `PalwStagedCourtRowV1` must gain the four new variants, and native heap/scan entries for `MaterialTargetV1`, `MaterialDraftV1`, `MaterialPartV1`, `MaterialRowV1`, `MaterialLedgerV1` (lane rule: generated files not edited here). Carriage preflight and the restart test depend on it.
- No wire tag / lifecycle dispatcher calls `material_intake` (same as the RFC-0029 shared fold it sits beside).

## Open / uncertain

- **Semantic tables (EOG / rendering / tokenizer)** are not intake targets: their per-claim byte requirement is not derivable from the original today (rendering widths are unbounded per entry apart from the 256 MiB file cap). They remain borrowed (`retain_semantic_table_material_v1`). User/design decision: how to bound them.
- **Payer and price** of storage (who is debited, refund on loss, per-byte price, TTL) — unadopted schedule; recorded only as `charged_sompi`.
- Per-submitter budgets bound storage by (#eligible bonds × requirement); real cost bound needs the price. Cover derivation is recomputed on every Parameter Begin/Finish (bounded by the schedule's cover budget; cost profile not measured).
- `RootedMaterialV1::load_v1` scans all request ordinals for existing query bodies (O(queries)).
- Merkle prompt tile width uses an upper bound on sibling depth; decode still verifies the opening.
- The "identical evidence" test uses `build_tir_cone_refutation_v1`; the shared-root/round `build_terminal` path takes the same store and was not separately exercised here.
- Multi-part bodies: the history fixture's parameter openings may fit one 16 KiB part; parts are still delivered in separate blocks. A wide fixture would exercise >1 part per body.
- U14 (averaging window) of lane M was not in this task.

Design decisions are within RFC-0032 §3/§4/§6–7 and ADR-0199 A (isolated implementation; no adoption of price, obligation or wire; missing → typed refusal, no slash).
