# Lane gdn-filer — G14 ③ / ⑦ (RFC-0006 layered Panel): the automatic GDN step filer

Status: **written, not compiled, not run** (lane rule: no cargo). Branch `g14p/gdn-filer`.

## What was implemented

| Piece | Where |
|---|---|
| Pure decision logic (detect → plan → object; window; reorg re-submit; conviction → false-Valid proof; book) | `kaspad/src/palw_gdn_step_filer.rs` |
| Detection: one probe = this node's own anchored step evidence (`base0_gdn_step_evidence_v2`) judged by the fold's own `palw_gdn_court_verdict_v2`; `ForgedCheckpoint` is left to the held filer (tag 244) | `palw_gdn_step_classify_v1`, `palw_gdn_step_probe_case_v1`, `palw_gdn_step_probe_window_v1` |
| Object: `GdnStepAccusedV2` built (`palw_gdn_step_accusation_v1`), re-judged by `palw_gdn_court_verdict_v2`, signed over `palw_gdn_court_session_id_v2` / `PALW_GDN_COURT_MLDSA87_CONTEXT_V2` only on `ExecutorGuilty` | `palw_gdn_step_object_v1` |
| Window vs. unpaid reward: mirrors the fold (tag 243 on a terminal claim only `Final` + accuser's live reservation); near DL-1's Final deadline reserve first (tag 154) when `palw_legacy_public_filer_v1` is in force; past it with no reservation → refused, logged with the exact DAAs | `palw_gdn_step_window_v1` |
| Reorg: reservation handed and Absent at tip after `PALW_GDN_STEP_RESEND_DAA_V1` → re-send (never a Closed one: once per life); accusation with no `CourtFraud` verdict after the interval → re-send; budget `PALW_GDN_STEP_MAX_SENDS_V1` | `palw_gdn_step_resend_v1` |
| Verdict tracking + ⑦: `Voided{CourtFraud}` → rooted `CourtFraud` proof noted in the false-Valid filer and woken | `palw_gdn_step_convicted_v1`, `palw_gdn_step_false_valid_proof_v1`; tick |
| Service tick (material session shared with the held filer, priority-lane submission via `court_outbox`, dispute-view reads, logging) | `kaspad/src/palw_panel/gdn_step_filer_tick.rs` `PalwPanelService::gdn_step_filer_tick_v1` |
| Shared entry discovery extracted from the held filer (behaviour unchanged) | `kaspad/src/palw_panel/gdn_filer_tick.rs` `gdn_filer_entries_v1`, `GdnFilerEntryV1` |
| Config flag, off by default | `PalwPanelConfig::gdn_step_filer`, `Args::palw_gdn_step_filer`, `--palw-gdn-step-filer` (args.rs, daemon.rs incl. drill extra-seat reset, palw_service_public_e2e.rs) |
| Panel loop wiring (after the held filer, before the false-Valid tick) | `kaspad/src/palw_panel.rs` (`gdn_step_book`) |

**FalseValid against every seat**: checked — `palw_false_valid_filings_v1` (consensus/core/src/palw_false_valid_filing_v1.rs) already enumerates every distinct `Valid` signer of the claim (not only one), pinned by the existing test `one_filing_per_seat_never_the_own_bond_and_only_what_the_chain_convicts`. No extension needed; this lane only feeds it the conviction.

## Tests added (crate `kaspad`, lib unit tests, module `palw_gdn_step_filer::tests`)

- `a_refused_or_honest_verdict_is_never_a_mismatch_and_a_forged_predecessor_is_the_held_filers`
- `the_probe_window_rotates_through_served_tiles_and_ends_empty`
- `the_window_files_inside_reserves_near_the_reward_and_refuses_after_it`
- `a_reorg_that_drops_the_reservation_resends_it_before_the_accusation_and_the_budget_bounds_it`
- `a_conviction_is_read_from_court_fraud_only_and_hands_the_rooted_contradiction_to_the_false_valid_filer`
- `the_book_opens_once_settles_for_good_and_is_bounded`
- `an_honest_producer_is_probed_to_the_end_and_never_accused` (uses the held filer's dev Qwen3.6 world; heavier)

Run: `cargo test -p kaspad --lib palw_gdn_step_filer`. Also re-run `palw_gdn_held_filer` and `gdn_filer_tick` tests (entry discovery was refactored).

## Fences

No new fence. Reuses `palw_memory_routing_court_v2` (owns tag 243; checked before filing) and, for the optional reservation, `palw_legacy_public_filer_v1`. To exercise on testnet-12 at DAA 16000 both need arming (plus `palw_shard_court` as parent of the memory-routing fence) and nodes started with `--palw-gdn-step-filer`.

## Design decisions (within adopted scope)

- Detection is sampled/sequential re-execution of the served step tiles + the court's own deterministic verdict (PRINCIPLES §8; RFC-0006 06-A pattern of `palw_gdn_held_filer`). No new evidence format; tag 243 as already defined.
- A step standing on a forged checkpoint is not accused here: it is the held filer's tag-244 case (avoids racing two courts).
- Final-deadline margin = 60 DAA and re-send interval = 60 DAA reuse the false-Valid filer's re-file interval; send budget 3. Node policy only.
- Persistence: in memory (as the held and false-Valid filers); restart re-detects and reads fates from the chain.

## Open / uncertain

1. Not compiled. Likely friction points: borrow of `book` across the `skip` closure/await in `gdn_step_filer_tick_v1`; NLL around `book.settle` while `sent`/`object` borrowed (paths `continue`/`break` right after).
2. The accusation carries ALL class inventory openings (as the held filer does); may exceed the court's `max_close_bytes` for a large class — then the fold refuses. A filter to the operands the refutation reads is not implemented.
3. Probing runs inline on the tick (like the held filer): one claim × 2 tiles per tick, but each probe is a prefix walk — needs measurement on the 8k row.
4. Discovery uses `palw_disputable_claims_v2` (ReceiptLicensed only); a case already Filed keeps being tracked past Final via the dispute view, but a claim first seen after Final is not probed.
5. The `ladder` passed to the local verdict is `material.max_leaves`, while the fold uses `class_step_ladder_v1` — equal for registered classes in the fixtures; unverified for held-ladder classes.
6. `an_honest_producer_is_probed_to_the_end_and_never_accused` assumes the inventory root equals the class `artifact_root` in the dev world.
