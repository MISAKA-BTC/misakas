# RFC-0026 F07 — dependency horizon source audit

Status: implementation investigation / Proposed integration contract, 2026-10-20. Based on the RFC14/G14 completion branch after `4f55a837d`. [RFC-0026 section 3.3](../../../rfc/0026-court-policy-master-contract.md) remains authoritative. No new ruleset, wire, preset or existing signed claim is changed by this record.

## Required bound and component correction

For original accepting-chain DAA `a`, dependencies must cover `max(a + 10,000 + required_followup, a + 10,000 + reorg_retention)`. All additions are checked. A shorter followup cannot hide longer material retention. The `CourtClockBounds` component previously checked only the followup expression, although it independently returned the longer material deadline. Its boundary test even admitted a dependency through 10,120 for material through 10,150. The corrected component rejects that case. Tests compare minus one / equality / plus one with retention longer, followup longer, equal and zero, preserving the original 3,120 / 10,000 windows.

A different claim on the same bond can extend collateral release beyond this bound. It does not require an unrelated dependency in this claim to acquire that other claim's scope. The component keeps `other_bond_liability` in the final maximum for bond exit, separate from the dependency minimum. It remains a component: scalar inputs do not prove that every real signed dependency was discovered.

## Authoritative code paths and traps

| Source | Actual meaning | Required use in new admission |
| --- | --- | --- |
| `palw_tir_public_fold_v1::prepare_original_v1` and `original_plan_v1` | Recompute the native original plan at actual accepting DAA, then authenticate the producer custody contract against source/resource plan and producer key | Reuse the same canonical geometry; do not use the miner's quote timestamp or overwrite the signed pledge |
| `palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1` and contract `check_v1` | Signed acceptance/material horizons; current check compares the plan material with the pledge material and acceptance strictly with accept_until | Check the new required material bound before any charge/install. Current coverage of the legacy plan alone is not F07 coverage |
| `palw_history_custody_v1::PalwHistoryCustodyPledgeV1`, `palw_tir_public_fold_v1::frame_semantic_bundle_v1` | Separate semantic/history pledge, present for the actual semantic component | The main TIR pledge cannot substitute for this pledge; check both when the semantic component exists |
| `palw_exec_v2_rewards_v1::horizon` / `exec_v2_liability_until_v1` | Maximum horizon for reward/collateral retention, including retained receipt | Never treat this maximum as the shortest signed dependency path |
| `palw_exec_v2_rewards_v1::require_native_session_window` | Minimum over the EXEC root floor and actual kernel prefix/slice dependency liabilities, plus panel and paid-consumer checks | Useful existing constraint, but its ordinary no-root/no-receipt case returns success; new admission must first establish the exact applicable dependency set, rather than interpreting absence as a covered path |
| `palw_provider_court_v1::ProviderLeaseRowV1` | Court lease state keyed by subject/provider, with reserved collateral, charged flag and serve_until; live_at rejects charged/released/expired rows | A horizon number alone cannot establish a live exact-subject service contract, and an arbitrary longer lease cannot stand in for a named required provider |

The above paths were read in this branch, not assumed from X3. `execution_root` on the original is not automatically an EXEC root identity. Do not reinterpret it to query a convenient long-lived receipt. The producer's material signature does not authenticate other workers' or providers' obligations.

## Remaining integration contract

The original's actual dependency graph must determine which EXEC roots/workers, exact provider subjects/leases, kernel liability rows and custody/history pledges apply. A complete admission candidate must enumerate all applicable references from authenticated original data/state, resolve each with its own signed scope and live status, reject missing or ambiguous required references, and use the minimum covered horizon. An absent role is permitted only when the original path demonstrably has no such dependency, not when lookup fails. A caller-supplied minimum or a signer assertion of completeness is insufficient.

Required_followup and retention must come from the adopted complete profile, including post-Final obligations and monetary material where applicable, not zero placeholders. Their actual production values and new effective-terms integration remain unadopted. No new manifest wire is chosen here. The atomic acceptance must validate all references and bounds before reservation, payment, installation or retention writes; a later failure must leave all of them unchanged. A claimant cannot extend another signer's horizon by creating an unresolved service hold. Old claims keep their own ruleset.

Node-level T09 must still exercise the shortest real lease, one long and one short dependency, missing/charged/expired/wrong-subject references, acceptance-delay rebasing, overflow, semantic plus main custody, same-bond multiple claims, reorg and carriage/restart/pruned IBD. Component boundary tests are not these acceptance tests. The current new admission caller is still missing; F07 and G14 are incomplete.

## Current original-route distinction and minimum binding guard

`palw_tir_public_fold_v1::prepare_original_v1` rejects `original.job.tail.is_some()` before preparing the frame. In this branch that excludes the Prefix/Gen/V5/Eval/Constraint tail routes, not just one particular tail. Registered semantic components are separately planned and must pass `frame_semantic_bundle_v1`. No claim is excluded here as a new completion shortcut: this is an existing limitation that still has to be resolved wherever the G14 coverage requirement includes those routes.

Consequently, walking `original.execution_root` as though it were an EXEC root would invent a dependency relation. EXEC and provider routes must be derived in their actual native admission paths. This investigation does not establish that an arbitrary TIR claim has all those roles, or that the broader G14 claim universe has none.

The candidate rooted service binding now applies a minimum real custody constraint in `CourtServiceState::check_context`: the accepted main pledge must cover the immutable original hard deadline. If the frame has a semantic component, its actual accepted ClaimBundle must exist; its signed history pledge must match claim, semantic resource identity, execution root, job pin and producer, and cover the same hard deadline. A missing bundle is rejected rather than treated as no dependency. This check runs on candidate row writes and snapshot-context validation through the existing binding validator. It never creates or extends a pledge and does not alter unbound old claims.

This is only the minimum hard-deadline guard. It does not certify paid physical material retention, the complete profile's followup/retention horizon, all EXEC/provider dependencies, original atomic admission or T09. It also does not give a service-held claim new signed obligations after the pledge expires. The native fixture checks the actual main pledge's boundary; semantic cases and the complete original admission still need dedicated integration evidence.

## Semantic guard test scope

A dedicated `rooted_service_semantic_custody_guard_rejects_missing_short_and_substituted_contracts` case exercises the contextual guard using the existing native main-frame fixture plus an explicitly injected semantic dependency. The semantic family plan is derived with the existing resource planner, but the composite frame/bundle is deliberately guard-only: it is not a signed native semantic-original admission or a serializable accepted snapshot fixture.

The case covers an absent bundle, semantic material at required minus one / equality / plus one, and independent substitutions of claim, resource plan, execution root, job pin and producer. It does not treat the main pledge's longer horizon as covering a shorter semantic pledge. Inserting rows directly isolates these checks from unrelated admission refusals. Success of this case must not be used as proof of native semantic original payment/signature/credit admission, dependency completeness, T09 or restart/pruned IBD. Those remain required.
