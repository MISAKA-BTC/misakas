# RFC-0029 native install / OwnerOpen / grant+Ready retained Source

Status: focused actual-consumer tests PASS (4/4); final integrated no-run build PASS. This record does not arm any preset, approve a claim, install a new source authority, or complete RFC-0029/RFC-0012 campaigns.

## Actual consumer changes

The three existing native public fold consumers now use the same retained constructor already connected to Output conviction: `install_original_v1`, `open_v1`, and `apply_grant_ready_inner_v1`. The existing registered record/layout lookup, full frame structure, descriptor class/layout, exact descriptor leaf ceiling, and source-ID checks remain intact. `Source::new_native_v1` supplies the actual reservation owner; its Resource variant reaches `PalwStateV2Error::NativeResource`, while Decode and identity faults remain semantic refusal. The owner stays in the actual callee through its last borrowed use by Preparation construction, owner binding, or native grant qualification, then drops before unrelated state writes.

A stack-only nonwire wrapper adds no heap allocation or authority. Test-only creation/drop/final-borrow counters supplement the actual quota controls; they are not evidence that a reservation was admitted. Legacy Source construction and other caller contracts remain available unchanged.

## Executed focused tests

Filter: `three_source_consumers` in `kaspa-consensus-core --lib` (four outer tests; the three quota tests each run one isolated child, counted within their parent).

- Actual plain original install: establish success, then find the constructor admission threshold under a named local 256 MiB test ceiling. One byte below must enter this consumer's Source constructor and return typed NativeResource, preserving state root and journal. Retry the same input with ample capacity; the full ceiling is reclaimable after all actual operation owners drop.
- Actual OwnerOpen: establish preparation through signed capture debit and signed PreparationReady, then use a real ML-DSA owner control at the existing preparation boundary. Apply the same source-stage refusal / rollback / retry checks.
- Actual grant+Ready: establish a real seat capture debit, then use independently valid ML-DSA receipt, raw grant, and outer grant+Ready signatures. First require this positive path to credit custody/duty/lock. Apply the same source-stage quota checks. An independently re-signed wrong-execution descriptor must remain semantic, roll back, and release its constructed Source owner; duplicate credit must refuse.

The fixture binds the producer key before job/context/execution/claim derivation and builds its frame with the existing public native frame constructor and actual signature verifier. Its preexisting accepted claim and drawn native panel are explicit fixture assumptions: this is not a mined transaction, selected-verifier approval campaign, real model capture, or public-chain activation test. No unadopted schedule or shared-material context installer is injected.

## Verification status and limits

Static parsing / formatting and `git diff --check` completed. Initial Core build failed with E0599 in the new test: the credit accessor was called on `frame.custody_v1()` instead of `frame.original_custody_v1()`. The parent corrected this test-only accessor before retry1. Initial failure log: `/tmp/misaka-rfc-three-source-core-build-20261025.log`. This initial compile failure is retained as history; final verification is recorded below.

The binary-search controls distinguish earlier builder/checkpoint refusal from this exact Source admission: the final refusal must observe one constructor attempt and no successful Source, as well as typed Resource and rollback. Counters at final borrow only supplement this, and the full-quota reacquisition checks actual release.

This scope covers the existing Source payload reservation and its lifetime. Derived Preparation/Owned/frame cloning, material intake, signing/serialization, allocator/runtime internals, and other legacy Source callers are separate resource boundaries. The ordinary legacy frame/source API, wire format, semantic caps, model tiers, activation None, normal bootstrap fences, and selected approval requirements do not change. Missing K3 inputs and fifteen campaign UNKNOWN gates stay unchanged.

## First runtime observation and fixture correction

Core retry1 build passed (3m01s), pinned binary `/tmp/misaka-rfc-three-source-core-retry1-20261025`, SHA256 `6bfdbd45935a56180a32ab20d20573ffe19b79ee34cac3795ba66bf7cc467ded`. Four tests ran: two passed, two failed (1.18s), log `/tmp/misaka-rfc-three-source-core-tests-retry1-20261025.log`. Plain install and signed OwnerOpen quota children passed (.17s / .48s); both grant tests failed before Source admission because the reused accepted-world fixture had no native panel duty row. This is the correct production refusal, not evidence of the grant Source path passing.

Test-only correction calls the existing native priced `reserve_seat_duties` writer on the fixture's actual accepted claim and actual Panel. It creates uncredited zero duties and reserves the actual exposure; no Ready, credited receipt, or lock is fabricated. The grant object must still create Ready, supplementary receipt credit, and S4 through the actual validators/writers. The corrected fixture passed all four final focused tests, as recorded below.

## Final verification

Integrated Core + Consensus (EVM) + Node (private/EVM) no-run build passed in 4m19s. Source snapshot: `/tmp/misaka-rfc-transition-header-three-source-wave-source-retry1-20261025` (16 Rust files across the integrated wave). Immutable Core binary: `/tmp/misaka-rfc-transition-header-three-source-core-retry1-20261025`, SHA256 `a1481164fec2bcc15aad9dcfc1fa5062915311cdc86ea9ea7f7e1f3b28dea800`.

`three_source_consumers`: **4/4 PASS**, 2.49s, log `/tmp/misaka-rfc-transition-header-three-source-core-tests-retry1-20261025.log`. The isolated quota children took .17s (install), 1.41s (grant+Ready), and .60s (OwnerOpen); these are included in the four parents, not additional independent tests. The corrected real duty writer fixture passes the positive triple-signature path, wrong-descriptor semantic rollback, duplicate-credit refusal, Source-stage typed quota refusal, same-input retry, and owner release controls.

Related regressions passed on the earlier immutable Core retry1 binary above, with identical Source production code and before the test-only duty-fixture correction: Output 2/2 (.25s, child .16s included), complete IDs 3/3 (.19s, children .17s/.00s included), staged preflight 2/2 (.22s, child .07s included), terminal assembly 1/1 (.15s), and service hold 1/1 (.14s). These prior-generation regressions are distinct from verification of the corrected new four-test fixture; they are not counted again as final-binary executions.

The first compile failure and the initial 2/4 runtime failure remain above. No production guard was relaxed to resolve either. The result is limited to these actual native consumer entrypoints and the admitted Source owner; it is not a live public-chain or all-resource campaign.
