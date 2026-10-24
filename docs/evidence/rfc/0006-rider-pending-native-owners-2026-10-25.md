# RFC-0006 rider pending-owner handoff — 2026-10-25

Status: final rider handoff runtime controls passed: `rider_guard` 8/8, overlapping CourtOutbox regression 12/12, and source FIFO/refusal transfer 1/1. Final node metadata passed, and the separate import-origin helper passed in both a direct source harness and the final consensus test binary. This extends the earlier prospective execution/signing connection through the real rider outbox and court carrier; it does not establish live-store/campaign acceptance, all-consumer completion, physical RSS bounds, or selected-verifier approval. Public activation remains None, normal bootstrap refusal and the arming validators remain unchanged. The 15 RFC-0012 gates remain UNKNOWN; formal K3 inputs remain uncreated.

## Actual handoff and publication

The producer now creates a bounded rider batch before allocating its rider signatures and public keys. Its checked typed request covers the batch/box, rider and guard arrays, and maximum native signature/key storage. The already-bound opaque native work Arc follows each successfully signed attempt into this batch. An original native None result instead retains the exact original whole job with its own reservation; it is not replaced by an inferred owner.

The in-process outbox moves the original boxed batch in FIFO order. Destination capacity refusal returns that same box to the source queue without allocating a replacement or losing its owner. Court admission reserves pending-array and due-map growth before allocation. The due map retains capacity after a batch leaves, so its ticket belongs to CourtOutbox, separately from the shorter-lived batch guard. Its checked request covers physical retained buckets rather than treating HashMap usable-entry capacity as bucket count; control bytes include the trailing group. The passing fill-then-clear HashMap quota regression covers this low-length/large-capacity case.

The court sidecar binds the complete ordered unsigned attempt IDs and lead rather than just the shared lead/round debounce key. These IDs include the actual output/execution roots and every unsigned attempt field. Signature witnesses intentionally remain outside attempt identity, as specified by the existing attempt protocol. The wire `AttemptRidersV1` representation is unchanged.

The sidecar holds original native owners and allocation tickets through pending storage, EDF drain, funding, async submission and retry. Pending pruning cannot release an in-flight owner. A canceled carrier keeps conservative ownership until the next flush reclaims its discarded payload. Actual carrier preparation or native submission refusal returns riders and owners to the existing retry buffer without stamping successful submission.

The actual carrier path refreshes before funding, again after asynchronous funding immediately before the signer, and again before native submission. Each refresh resolves the routing fence from own native Params and the session's current virtual DAA, rather than using the earlier tick's clock. Original Some owners use the dedicated native refresh door, preserving pinned canonical ancestry, import, exact work/profile/router identity and current eligibility. Original None jobs call the own native rider factory again and must still return None; a later Some result requires reexecution rather than manufacturing an original execution owner. Missing original decisions cannot pass an active fence. Typed Resource and Unavailable failures keep the pending execution for retry. Identity refusal retires this local pending execution at all three guard doors without a submission stamp, claim verdict or monetary penalty; another execution is required. In particular, original None becoming Some and an active missing original marker/sidecar invalidate the pending path rather than waiting forever. Native read failures, active/unstable import and reversible noncanonical-ancestry observations remain Unavailable and retryable. A completed import-epoch mismatch invalidates the original local execution and is Identity, requiring reexecution; it is not a claim conviction or monetary penalty. Generic carrier build/submission errors are conservatively retried and are not classified by their text. None of these outcomes supplies arithmetic evidence or fabricated approval.

Nothing in this queue or sidecar is serialized. Restart loses the in-process pending batch, as before. A recovered wire object cannot reconstruct its opaque owner; active routing requires a fresh authorized execution path.

## Focused verification

The first parent-run `rider_guard` filter completed with **3 PASS / 1 FAIL**, 0.25 s, log `/tmp/misaka-rfc-rider-guard-outbox-tests-20261025.log`. The failed Some(owner) fixture used a grouped-reader name without the native `.routed` suffix, so strict carriage restoration correctly refused its declaration as an inexact routed-reader inventory. The fixture now declares `test.routed`; native registration/inventory validation was not weakened. The first compile is not final-generation evidence, and the failed control is not counted as passing.

The corrected parent-run build completed in 6m14s including lock wait. The same verified binary `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4` ran:

| Scope | Actual result | Log |
| --- | --- | --- |
| Rider guard identity, original Some/None, typed disposition and capacity ownership | 8/8 PASS, 0.23 s | `/tmp/misaka-rfc-rider-guard-outbox-tests-retry1-20261025.log` |
| CourtOutbox regression, overlapping the rider group | 12/12 PASS, 0.40 s | `/tmp/misaka-rfc-rider-court-outbox-regression-tests-20261025.log` |
| Source refusal/FIFO transfer | 1/1 PASS, 0.00 s | `/tmp/misaka-rfc-rider-source-transfer-tests-20261025.log` |

The rider group includes its two isolated one-test child processes (0.00 s each); they are not extra tests added to the 8/8 count. The 12/12 CourtOutbox group overlaps those controls and is not summed into a unique-test total. These passing controls exercise:

- Exact ordered attempt binding rejects reordered or changed-output-root batches, while preserving the protocol's signature-witness identity rule.
- Active missing-owner refusal does not synthesize native authority; dormant legacy fixtures remain readable. Original None retains the exact whole job/origin, rechecks native None, and rejects a new Some result as an invalidated pending execution.
- Source import refusal preserves the same box and FIFO order.
- An isolated demand-budget control exercises destination refusal, pending/flight/retry/cancellation retention and final ticket release. Due-map capacity remains owned after batch retirement until CourtOutbox drops.
- A Some(owner) fixture uses public native state transitions, strict carriage restoration and the real prospective rider factory. Its real outbox/CourtOutbox handoff control checks that the refresh callback sees the exact original Arc through pending/flight/retry, preserves a typed Resource refusal, refuses a changed batch, and releases the original only after the last payload retires. Its state rows are synthetic; this is not a live native-store or cryptographic approval test.

Two further controls exercise the actual pending-refusal disposition: Resource/Unavailable survive real CourtOutbox drain/retry with the exact original Some Arc, while Identity releases it after final payload retirement without marking submitted. The original-None control feeds the retained whole job/origin to the production result guard and refuses current Some/missing active markers; unchanged None remains supported. These controls passed in the final rider/CourtOutbox groups above.

The actual parent-run filters were `rider_guard`, `court_outbox` and `rider_source_import_refusal`. Final node metadata passed. The later import-origin typed helper passed in a separate direct source harness; that helper execution does not establish a real native-store import/reorg campaign. No Cargo job, deployment, transfer or campaign was started by this agent. Syntax and `git diff --check` passed before handoff; these do not substitute for compilation or runtime results.

## Remaining limits

Native store/branch-change and expiry behavior still need integrated tests, followed by the real campaign. The retained batch ticket covers its enumerated batch/signature/guard storage; existing generic carrier serialization, funding and transport scratch are separate resource boundaries and are not silently covered by that ticket. Missing accepted-rider anchor callers, generic recovery/fraud/partial-resume routing, inherited-prefix descriptors, no-capture exact counts, genesis registrant authority, all-model costs and whole-node capacity remain separate requirements. A matching execution/root does not replace randomly selected verifier approval or change the frozen model tiers.

Final parent-run node library/tests/private-node metadata check: **PASS**, 8m52s including lock wait, `/tmp/misaka-rfc-provider-rider-header-node-check-20261025.log`. This does not turn the fixture scopes into a live-store campaign.

The actual native origin decision and its exact production test were extracted verbatim into a small source harness and ran **1/1 PASS**. It checks that a completed import epoch mismatch is Identity/reexecution while a noncanonical ancestry observation remains Unavailable. Source/fragment hashes and exact compiler command: `/tmp/misaka-rfc-routing-import-identity-source-build-20261025.json`; runtime log: `/tmp/misaka-rfc-routing-import-identity-source-runtime-20261025.log`. This is helper execution against cached core types, not a full native-store import/reorg execution. The full production integration also typechecks in the node metadata check.

The final full consensus test binary subsequently executed that same native-origin helper control: **1/1 PASS in 0.02s**, `/tmp/misaka-rfc-routing-import-identity-native-tests-20261025.log`. This supersedes the harness-only validation limit for compilation/runtime of the helper, while preserving the earlier harness record. It still does not exercise a live native-store reorg or import campaign.

Implementation source commit: `5824a7e4f`.
