# Session-owned multipart DA answer checkpoint — 2026-10-11

This checkpoint implements the dormant consensus transport foundation for G14. It does not complete G14 and does not arm the new objects. ADR-0177 D7 continues to assume a verifier holding the correct registered model.

## Implemented

Objects 160/161 carry a signed manifest and authenticated parts owned by an existing, unfulfilled public DA session. They use separate pending-answer state rather than the eight certification groups. Admission fixes the deadline to the existing session, bounds allocation before signing/serialization, rejects replacement and malformed parts, and permits completion only after checking the assembled original answer signature and the existing court authentication. State roots, snapshot tail, apply/revert journal, cleanup and carrier classification include the pending answers.

## Verification at this checkpoint

- 59 unique core/node tests passed: the earlier 38 core, 15 legacy node, three carrier and one real MLDSA gate tests; one additional maximum-32-part boundary test; and one repaired V-fold completion/default/restart regression. Repeated tests are not counted again.
- The maximum-32-part test checks each carrier fits, reverse arrival completes exactly once, and a one-byte-smaller envelope budget fails before signing.
- The V-fold regression exercises all eight occupied certification groups, partial snapshot restart, malformed snapshot rejection, duplicate/corrupt parts, direct-answer cleanup, multipart completion and withholding default with producer forfeiture. It also checks that an overly fragmented manifest is refused when the existing DA window cannot carry it.
- Shipping integration harvest completed successfully. The mechanical shipping comparison reports 361 pins OK, no drift, all required gates passed and no unregistered literals. These results are recorded in [shipping tests](evidence/owned-answer-shipping-tests.log) and [pin comparison](evidence/owned-answer-shipping-pins.log).
- Final core/node test builds and the three-part test log are preserved alongside the prior check, gate and regression logs. Initial compiler/test failures are preserved as historical evidence; corrected runs are explicitly named repaired/boundary/window.
- The H1 gate uses real MLDSA signatures but synthetic pending-answer state. It is not an end-to-end real service proof of this multipart transport. The V-fold test uses its existing licensed fixture; it is not a maximum-profile service benchmark.

## Remaining implementation

1. Producer automatic splitting, scheduling and restart continuation against accepted session progress.
2. Bounded reconstruction of authenticated multipart answers by a fresh verifier reading public history; missing/pruned history must lead to the existing reacquisition court.
3. Full admission/profile alignment for maximum job/input sizes and court byte/time/memory budgets. The existing 64 KiB job limit is unchanged, and 32 parts are not a proof of coverage for every active profile.
4. Initial/prefix state and non-prompt material, all active computation/output/state families through normal admission, cumulative scope/hiding, inclusion/resource bounds, reorg/pruned IBD/RPC/P2P and activation/economic/soundness gates.

Until those conditions are closed, arming remains fail-closed and full G14 remains unfinished.

## Producer continuation — 2026-10-11 (verified checkpoint)

The producer/covering-signer worker now obtains a bounded native progress view (signed manifest, fixed deadline, 32-bit accepted-part bitmap). Reader failure is distinct from absence. Large legacy answers are planned as a manifest and then one missing part per carrier. Restart rebuilds and deterministic signing must reproduce the complete accepted declaration; mismatched envelopes, foreign deadlines, out-of-range bitmaps and expired rows are refused. Queue keys distinguish stages, and obsolete declarations/accepted parts leave the queue. The native progress view is local consensus state, not an RPC/P2P state proof.

The common signed-answer builder counts the complete fixed-size MLDSA envelope before signing for multipart transport. Replay reservations remain held through transport planning; additional temporary serialization/signature scratch and outgoing-carrier bytes are reserved for legacy units. Hashing, signing and splitting run off the async loop. This does not close the outstanding global queue/state memory audit.

Current production `cargo check -p kaspad --lib --locked` passed. The final test build was rerun against unchanged source and completed successfully. New planner tests (2), existing legacy V-fold tests including resumed completion (16), DA policy tests (9), and the held-builder route check (1) passed: 28 unique tests in this continuation. The separate partial-restart test run overlaps the 16 and is not added again.

The real outsider-service regression for unretained FP job/input acquisition, independent replay and release also passed (303.37 s, release observed at DAA 1186). Its fresh database validated 2065 public blocks before starting the service, and original job body/acceptance rows and saved rooted job metadata were absent. The test asserts the actual replay worker judged the roots Honest before accepting release. This brings this continuation to 29 unique PASS. That fixture manually supplies authenticated producer carriers, so it does not prove the new producer multipart worker end to end.

Logs: [final normal library check](evidence/owned-answer-producer-final-check.log), [final unchanged-source test build](evidence/owned-answer-producer-final-test-build.log), [planner](evidence/owned-answer-producer-tests.log), [legacy V-fold](evidence/owned-answer-producer-legacy-regression.log), [DA policy](evidence/owned-answer-producer-regression-tests.log), [held builder route](evidence/owned-answer-producer-builder-route.log), [real outsider service](evidence/owned-answer-producer-service-honest.log). Initial missing proxy-forwarder/incorrect map-type compiler errors are preserved in the initial check log; the repaired/final checks passed.

The next dependencies remain: replace the current single-carrier demand/reacquisition fit assumption with an exact signed multipart envelope/window bound; justify the FP job admission ceiling against ordinarily admitted job bytes; implement bounded authenticated history reconstruction; and run maximum-profile producer/outsider service tests. The existing 64 KiB job cap is unchanged. Full G14, activation and all-family/profile coverage remain unfinished. Shipping pins were verified at the preceding consensus checkpoint; the shipping harvest has not been rerun for this native-reader/node-policy continuation.

## Input/job multipart admission — 2026-10-11 (interim push)

At the user's request, this source checkpoint is pushed before the current verification round finishes. It replaces the single-carrier demand/reacquisition assumption with a shared bound for the complete signed answer, the 32-part transport ceiling, the court byte ceiling and the existing DA window, including the declaration carrier. Both processor admission and core session opening/reacquisition use the transport bound.

The FP job retrieval ceiling is now 125,000 bytes. For the currently shipped presets, ordinary non-coinbase transaction transient mass is estimated bytes times four and cannot exceed the 500,000 block-mass limit. The job is contained inside that transaction payload. This justification does not cover custom presets with larger mass limits. A canonical constraint/job preimage above the former 64 KiB retrieval limit passed the new authentication test; this is not a full ordinary-admission or large-job service test.

The node test build completed successfully. The initial core test run failed the new Flat sizing fixture because it used a held profile, whose class forces Merkle input. The fixture was corrected to use the regular profile; its rebuilt test and the updated real-signature admission gate were still pending at the time this checkpoint was prepared. The previous checkpoint's 29 PASS must not be treated as verification of these new admission changes.

Public multipart history reconstruction, maximum-profile producer/outsider service tests, the current shipping verification round, and the remaining full G14 guarantees are unfinished. Activation remains fail-closed. ADR-0177 D7's registered-model possession assumption is unchanged.
