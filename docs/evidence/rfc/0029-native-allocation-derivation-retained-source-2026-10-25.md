# RFC-0029 native allocation derivation retained Source

Status: integrated no-run build PASS and focused/regression runtime PASS. No activation, new authority, wire change or complete-preclaim-resource claim.

## Actual native callers and legacy compatibility

`original_plan_v1` now uses additive `derive_tir_public_allocation_native_v2`. The typed Frame entrypoint uses additive `check_tir_public_allocation_retained_v2`, which rederives through the same native derivation. Actual FP paired preparation and native original/preclaim quotes therefore reach retained Source construction inside the earlier allocation stage. The fixed caller supplies its existing registered class/layout, header, exact job, geometry and native rules.

Legacy `derive_tir_public_allocation_v1` and `check_tir_public_allocation_native_v1` retain their raw Source constructor / String API. They do not stringify the new typed Resource result. Both derivation variants keep the original preclaim-identity check before Source construction, then share one private finishing routine for all existing view, geometry, semantic-family, work, terminal, full-SUM clock/cost, source-ID and exact plan checks. Native Resource maps to the existing outer NativeResource retry; semantic Decode remains the original refusal, and Incomplete is not reclassified as local capacity.

The native Source uses existing `native_construction_requests_v1` and its actual retained ticket. It remains alive through the common plan derivation, including the last source-ID field use. It is dropped when the derivation returns its plan/Incomplete/error. No Source pointer or ticket becomes serialized authority. Complete plan equality and all existing caps are unchanged.

## Executed tests and scope

New filter `allocation_source_owned`: one outer test with one isolated child included in its count. Existing `preparation_sources_owned`: two outer tests, each isolated child included.

The new direct derivation control reuses the real signed, fresh unclaimed funded-candidate fixture. It first establishes exact raw/native complete-plan equality. One byte below the actual Source quote must enter the retained constructor and fail typed Resource, while the old raw entrypoint retains its original behavior. With exactly that quote available, actual derivation succeeds. A test-only final-borrow observer performs an actual one-byte reservation attempt while the real Source owner is still alive and requires refusal; after return the full Source quote is reclaimable. The observer cannot supply an answer, change authentication, or inject Resource into the result. Changed trace root or carried source identity stays semantic, and original input retries. Full local quota is reclaimable after operations.

The prior preparation tests are updated to the new actual ordering. Whole paired/preclaim one-byte-below refusal now targets the earliest allocation Source (one derivation attempt, no successful owner, no later direct Source attempts), rather than pretending to reach the old first direct constructor. The direct typed Frame `q-1` refusal now also occurs inside allocation derivation, before its later Source constructor. Its exact-`q` successful actual signature callback still verifies the second Source owner's lifetime. These refusal and successful-lifetime observations are separate; no fake later-stage Resource is injected to preserve an obsolete assertion.

## Limits

This covers Source construction and its actual owner lifetime only. The common derivation still separately constructs a decode view, registered binding / verified StepSpace, semantic geometry and plan/hash writers through existing APIs. Frame argument clones, header-digest scratch, Preparation/Owned/frame construction and allocator/runtime internals are separate resource roles. Existing TransitionBuilder state/checkpoint and staged journal reservations are not double-reserved here. This result must not be described as all preclaim heap coverage.

The accepted-class fixture, selected approvals, real model capture, mined payment/admission and public chain campaigns are not created by these local controls. Normal activation remains None, bootstrap/arming fences stay unchanged, K3 remains absent, and fifteen campaign gates remain UNKNOWN.

## Final runtime evidence

The integrated Core / Consensus / Node (private/EVM) no-run build passed in 4m35s. Frozen 28-file source identity record: `/tmp/misaka-rfc-snapshot-allocation-wave-source-20261025/record.json`. Immutable Core binary: `/tmp/misaka-rfc-snapshot-allocation-core-20261025`, SHA256 `913d77a6dd8ed772513d476ac7c135cd2518b452927ed3624a0a8c7a04883bd0`.

- New `allocation_source_owned`: **1/1 PASS**, .16s (isolated child .15s included), `/tmp/misaka-rfc-snapshot-allocation-derive-tests-20261025.log`.
- Updated `preparation_sources_owned`: **2/2 PASS**, .79s (children .18s/.59s included), `/tmp/misaka-rfc-snapshot-allocation-preparation-tests-20261025.log`.
- Existing allocation module regression: **7/7 PASS**, .49s, `/tmp/misaka-rfc-snapshot-allocation-allocation-regression-20261025.log`.
- Source-three-consumer regression: **4/4 PASS**, 1.86s, `/tmp/misaka-rfc-snapshot-allocation-source3-regression-20261025.log`.
- Output regression: **2/2 PASS**, .22s, `/tmp/misaka-rfc-snapshot-allocation-output-regression-20261025.log`.
- Funded-candidate generator regression: **1/1 PASS**, 2.57s, `/tmp/misaka-rfc-snapshot-allocation-funded-regression-20261025.log`.

Children are included in parent counts. The updated controls confirm the actual earlier allocation refusal stage, rather than reporting later direct Source refusal. All results are scoped to this immutable binary and the existing tiny actual native fixture; no live model/campaign or all-resource coverage follows.

Read-only follow-up identified the initial native `header.clone()` preceding Source admission as a separate concrete construction boundary. It is not covered retrospectively by this Source lease and remains for a later change. Later geometry needs an explicit simultaneous-role comparison against the Source constructor peak before adding reservations, to avoid double counting.
