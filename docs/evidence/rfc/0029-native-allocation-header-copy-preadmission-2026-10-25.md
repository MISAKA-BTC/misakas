# RFC-0029 native allocation identity header copy preadmission

Status: focused retry runtime PASS (2/2); integrated no-run build PASS. This is one actual construction boundary, not full preclaim heap coverage or activation.

## Actual caller and lifetime

The native allocation derivative previously deep-cloned `header` into a temporary `PalwPublicExecutionBindingHeaderV1::Tir` before the retained Source admission. The native identity checker consumed this enum. Source's later reservation could not retrospectively authorize that earlier clone.

The native entrypoint now uses existing `NativeRetainedV1::try_from_clone_v1` on the actual typed TIR header before copying. It moves the admitted payload into the same enum without another clone, runs the unchanged identity checker, then explicitly drops enum payload before its ticket. Both drop before Source construction, including on semantic identity refusal. If identity or observation panics, reverse declaration order likewise keeps payload drop before ticket. Legacy raw/String derivation is unchanged.

The quote is the existing typed `native_value_requests_v1(header)` used by the clone owner, including actual nested job-context Vec fields; no serialized-length heap approximation or new policy cap is used. The existing metadata-envelope/semantic checks keep their order within the identity checker. This change reserves the copy before that checker without granting trust to the copied header.

## Executed controls

Filter `native_header_identity` in Core lib: two outer tests, each one isolated child (child included in parent count). The actual signed fresh funded-candidate fixture and native plan are reused.

- Direct native derivation with one byte less than the exact header quote must refuse typed Resource at header admission before copying or entering Source. With exactly the header quote, a read-only test observer at the actual identity use checks that a real extra one-byte reservation refuses. Header identity succeeds; the much larger Source quote then refuses independently. This is not recorded as a complete-plan success. At return the entire header quote is reclaimable. A changed execution identity is semantic, enters no Source, and releases its header owner; the correct input retries to the exact original plan with ample capacity.
- Actual paired preparation arms quota before TransitionBuilder construction and finds the exact threshold for header-copy admission after builder creation. One byte below must have one header attempt, no copy, and no Source attempt; the caller returns outer NativeResource with identical state root/journal. Ample same-input retry succeeds, and actual native preclaim quote returns the identical plan.

Stage counters are observation only. The test-only observer cannot alter authentication/results or inject Resource. Actual reservation failure while the admitted clone is borrowed and full-quota recovery provide owner evidence. Other quota tests still target Source stages truthfully: the header copy is released first, and this fixture's Source quote is larger than the header quote, so no permanent extra lease is carried into those stages.

## Remaining boundaries

The identity check's existing job-ID/hash writer scratch, later Source/derivation geometry, frame argument/header-digest/Preparation/Owned construction, and allocator/runtime internals remain separate roles. Source constructor peak must be compared with later simultaneous geometry before adding another reservation; this copy fix does not double count the existing Source quote. No claim, selected approval, shared-material installer, or new admission authority is created.

Wire/schema/source identities, model tiers, all public activation None/arming refusal, bootstrap fences, K3 absence and campaign UNKNOWN gates remain unchanged.

## Initial runtime failure and test correction

Integrated no-run build passed (4m44s), but the first `native_header_identity` execution failed both parents before exercising quota admission. Log: `/tmp/misaka-rfc-header-state-import-header-tests-20261025.log`. The test requested `native_value_requests_v1(header)` while admission was still off. Its actual helper contract deliberately returns zero in that state (`retained.rs::value_requests_v1` checks enabled admission before quoting), so the positive header-quote ordering assertion failed. This is a test setup error, not observed production refusal or passing owner evidence.

Test-only correction arms the named local logical ceiling before obtaining the actual header quote. It logs mode, ceiling, before-arm header quote, after-arm header quote and Source quote into the runtime artifact so the actual values/order are reviewable. Production owner construction and semantics remain unchanged. Corrected build/runtime passed as recorded below; the initial failure remains recorded.

## Final verification and artifact generations

The corrected three-package no-run build passed in 1m34s. Retry source identity record: `/tmp/misaka-rfc-header-state-import-wave-source-retry1-20261025/record.json` (10 files). Immutable retry Core binary: `/tmp/misaka-rfc-header-state-import-core-retry1-20261025`, SHA256 `4504084863c6baeedf2784f9fdb81a143433ba6a3ec8b6549b722c838772fd34`.

`native_header_identity`: **2/2 PASS**, .47s, log `/tmp/misaka-rfc-header-state-import-header-tests-retry1-20261025.log`. Actual caller child .35s and direct copy child .11s are included in the two parents. Both logged the actual quote order: admission-off header quote **0**, admission-on header quote **1116**, Source quote **217440**, named local ceiling **268435456** bytes. The observations verify actual header-stage refusal and lifetime/release; zero was the disabled helper result, not a zero-cost native copy.

Related regressions passed on the first build's immutable Core artifact, SHA256 `aa3b55f8e89c9dcc084901f03b501923e76464063b84c4672dfdfbe0d081186e`, with identical production code and before the test-only quote-order correction:

- Derivation 1/1, .12s: `/tmp/misaka-rfc-header-state-import-derive-regression-20261025.log`.
- Preparation 2/2, .76s: `/tmp/misaka-rfc-header-state-import-preparation-regression-20261025.log`.
- Allocation 7/7, .49s: `/tmp/misaka-rfc-header-state-import-allocation-regression-20261025.log`.
- Source3 4/4, 1.76s: `/tmp/misaka-rfc-header-state-import-source3-regression-20261025.log`.
- Output 2/2, .21s: `/tmp/misaka-rfc-header-state-import-output-regression-20261025.log`.
- Funded candidate 1/1, 2.45s: `/tmp/misaka-rfc-header-state-import-funded-regression-20261025.log`.

These first-generation regression results are distinct from the corrected retry's two focused tests. Child processes are included in parent counts, and the first failed focused execution is not counted as PASS. No production guard was relaxed; the narrower copy reservation and all explicit remaining boundaries above remain the scope.
