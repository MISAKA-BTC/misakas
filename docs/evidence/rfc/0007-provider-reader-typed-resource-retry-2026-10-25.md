# RFC-0007 actual provider reader typed resource retry — 2026-10-25

## Repaired production boundary

The private `kernel_paid_checker_v4` already dispatches actual accepted target authority through
`VerifiedPaidClaimSourcesV2` into Program/Pipeline, TypedSpec and Segmented provider readers.
Its transport preserves `LocalCapacity`, `Unavailable`, `InvalidMaterial` and `Unknown`, but the
SDK reader converted its own quota/decode failures into generic `Refusal` and then sticky
strings. The caller withheld all actions safely, yet could no longer identify local SDK
capacity as a retry cause. This change preserves that classification at the actual consumer;
it does not introduce a new direct-check or approval mode.

The additive classified reader and parameter APIs use the existing typed material fetch error.
Legacy String transport / Refusal APIs remain wrappers. Their opaque error strings become
Unknown; display text is never parsed to infer resource exhaustion or provider misconduct.
Optional material traits retain the first typed error and stop further queries. A failed read
cannot return a Clean finding or completed selected-seat verification result.

Local explicit workspace/request limits, native admission/retained decode, parameter tensor
construction and segmented assembly keep LocalCapacity. Structural decode failures retain
InvalidMaterial; accepted ledger projection uses its checked getter, with Resource preserved
and semantic projection failure left Unknown. The authentication quote uses the existing native
parameter/position/segmented request functions under classified retained temporary decoders;
it avoids the historical core String adapter's erasure of its own admission failures. Existing
opaque structural quote errors remain Unknown rather than being classified from text.

The actual checker now calls the classified API. LocalCapacity takes an explicit wait/retry
branch without constructing or signing any provider obligation, refutation or receipt. Remote
Unavailable/InvalidMaterial retains the existing monetary response-obligation path only when
an exact failed transport query was recorded. A local SDK decode error alone does not create
that query authority. Existing WAL, native authority, consent generation, challenge deadline,
funding, rehearsal and selected-seat checks remain in place.

## Verification status

The coordinated SDK retry passed the focused new filter **4/4** on 2026-10-25 in 0.17
seconds. Its isolated quota child passed in 0.03 seconds and is included within those four
cases. The build took 6m26s including shared-target lock waits; it is not a model cost
measurement. Log: `/tmp/misaka-rfc-provider-reader-typed-tests-retry1-20261025.log`.

The same actual SDK unit binary subsequently passed the full provider-reader regression group
**11/11** in 1.93 seconds, including the four new cases, in
`/tmp/misaka-rfc-provider-reader-regression-tests-20261025.log`. These overlapping runs are
not fifteen distinct tests. Binary:
`/tmp/misaka-rfc06-12-build-20261024/debug/deps/misaka_palw_sdk-67914e9d9b94cc3f`.

The initial node build compiled the lib/private source, but an unrelated rider fixture failed
at runtime. Final node private-bin metadata validation subsequently passed after that separate
fix; no whole-node campaign pass is inferred from the SDK result. No independent Cargo job
was started. Scoped `git diff --check` also passed.

The first SDK test build failed with two E0599 method-resolution errors: an existing direct
Reader fixture still declared a String-returning fetch closure after the internal reader became
typed (`/tmp/misaka-rfc-provider-reader-typed-tests-20261025.log`). Its closure return type was
corrected without changing the optional mutable-slot assertions. The exact native scanner
arithmetic overflow kind (`OutOfMemory`) is also classified as LocalCapacity alongside
Unsupported; a control invokes actual `NativeWireShapeV1` overflow without allocation, while
malformed actual provider bytes remain InvalidMaterial. No error display text is classified.
The corrected SDK retry and regression results are recorded above.

The focused SDK filter is
`claim_paid_v2::provider_reader_v4::memory_tests::resource_tests` (four tests), alongside the
existing provider-reader regression group. The new controls use the real small Memory
producer/provider/checker fixture to exercise explicit local workspace refusal followed by a
fresh successful complete retry, sticky transport cause preservation, malformed unit refusal,
and legacy opaque-string compatibility. An isolated child arms actual zero quota only after
fixture construction and checks native parameter-authentication admission and selected
material admission. Child execution is included within its parent test count.

These are bounded mechanism controls. The four-adapter native campaign, all-stage/all-model
cost measurement, independent source premises and K3 manifest/weights/signed execution inputs
remain open. Public activation stays None, normal selected-source/V6 arming refusals stay in
place, and model tiers/claim authentication are unchanged. Matching local replay does not
replace selected-verifier approval.

Final parent-run node library/tests/private-node metadata check: **PASS**, 8m52s including lock wait, `/tmp/misaka-rfc-provider-rider-header-node-check-20261025.log`. This does not turn the fixture scopes into a live-store campaign.

Implementation source commit: `c0db0bffc`.
