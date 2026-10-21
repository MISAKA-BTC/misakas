# PALW behavior-preserving refactoring plan

Status: In progress. User changed the active task from G14 feature work to completion of the refactoring described in two attached reviews. Base `292a6d7db`; branch `codex/palw-behavior-preserving-refactor`, reusing the clean attached integration worktree. Original checkout modifications are not part of this work. Prior G14 commits remain on their original branch; this task adds no G14 feature or monetary rule.

## Full requested scope and acceptance

| Area | Required result | Current evidence/status |
| --- | --- | --- |
| 0 Baseline | Map canonical, historical, dormant and deferred paths; record starting test results and identity inputs | Started: three targeted preparation/fence baseline tests and executable hash saved. Full route matrix pending |
| 1 Court send state | One typed owner for pending/due/moved, preserving deadline order, duplicate suppression, submitted vs accepted and retry/recovery | Pending; current worker and child pass inputs inventoried |
| 2 Carrier and fee ownership | One build/submit/change-outpoint persistence/funding/inflight boundary; unchanged success/failure/restart behavior | Pending |
| 3 Consensus preparation | Acceptance and full transition share pre-object work; historical ordering, roots, deltas and native resource errors preserved | Shared helper extracted; 38 existing tests match before/after, 29 integration and 3 transformer-pin tests pass. Full phase/remaining structural work is not inferred complete |
| 4 Rules and views | Strengthen existing canonical rule resolver; separate block facts/current rules/claim acceptance rules, preserving accepted_daa semantics | Pending; no parallel resolver to be introduced |
| 5 Runtime/validator/backend responsibilities | Central worker becomes orchestration; validation dispatch and capability interfaces depend on narrow inputs, ordered side effects retained | Pending |
| 6 Compatibility paths | Classify normal/history/dormant/deferred/unused entrances; isolate misleading nonproduction sync APIs, retain necessary old wire and replay | Pending |

Completion requires all areas, not just the first extraction or shorter files. Verify acceptance/refusal/exclusion, bytes/IDs/state roots/deltas, collateral/reservation/release/slash/reward/weight, activation boundaries, claim-time rules, restart/reorg/pruning and submit/retry behavior at each changed boundary. Preserve independent reference implementations and journal/atomicity. Build/test commands use the existing heavy lock. No production arming, pre merge/push, deployment, new bond/reward/fee/deadline/cap or activation change is authorized by this structural task.

## Consensus preparation finding

The two pre-object regions differ operationally in artifact ownership migration placement: acceptance places it before class retarget; full application places it after vertex/mesh sweeps. The pre-object entry also rearms material retirements after the shared region, whereas full application rearms later after its object/work processing. No claim that these orders commute has been established. The refactor preserves both historic positions explicitly rather than silently selecting one as canonical.

`palw_state_v2/block_preparation.rs` now owns all common ordered statements. An internal migration-order enum records the two old placements. Entry-specific rearm stays at its original call site. A normalized statement-sequence comparison projects both enum choices and verifies equality to each old region; this is structural evidence, not a replacement for runtime tests. The existing fold source-reading API includes the new child so guards still see all production code.

## Fixed compatibility constraints

Keep wire enum ordinal/discriminant order, legacy entry behavior and codec bytes; Final and SecurityCertified remain distinct. Do not move or edit `misaka/palw/derive/src` because transformer identity hashes paths and source content. Preserve its pin and independent reference implementations. No crypto computation proofs or model-level review are introduced; the permanent two-tier premise and deferred media/improvement paths remain fixed.

Evidence: [baseline and preparation sequence audit](../../../evidence/palw/refactor-2026-10-21/baseline.json). Source reviews were static and their numerical size counts are not treated as fresh measurements of this checkout. The two review attachments are task inputs, not instructions to import their external URLs or unexecuted claims as verified facts.

## Preparation checkpoint evidence

The unchanged pre-refactor executable was preserved with its recorded SHA-256. Migration/model-line, reward-sweep, claim pre-object judgment and retained/legacy canonical-byte tests pass under both binaries with the same 38 unique test names/results. Fresh integration tests cover carrier refunds (5), deadline re-date (8), burn/cap acceptance vs fold (2), t12 identity (9) and activation (5). Transformer pin tests pass 3. Total current selected passes: 70; repeated pre-object/baseline subsets are not added to that total. These are scoped existing assertions, not a claim to compare every possible state/root/delta.

Remaining work is still the full court/fee ownership, rule/view and validator/runtime/backend boundary, compatibility-route classification and complete regression/recovery matrix. No endpoint or frozen semantic rule has been removed or normalized.
