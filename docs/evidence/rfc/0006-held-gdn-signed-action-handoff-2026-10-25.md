# RFC-0006 retained held-GDN signed-action handoff — 2026-10-25

## Actual production consumer

The held-GDN step previously cloned its binding/operand openings, signed and boxed raw demand
or accusation objects, then advanced its demand clock or marked the claim accused before the
Service caller's raw outbox enqueue. Retained capture admission covered inputs, not this output.

The new retained step pre-admits output storage before binding/opening copies, signature and
Box construction. Demand admission derives binding heap plus its actual typed canonical
message-field lengths; accusation admission derives evidence ownership and every copied opening's
heap/slot storage. Native ML-DSA-87 bounds the signature allocation retained by the result; an
oversized returned length/capacity refuses. Memory-routing session hashing already streams fixed
fields and needs no fabricated prefix bound. The result is an immutable native retained object.
The public `retain_existing_v1` API acquires the completed action's exact independent ticket
before releasing the earlier construction reservation; that brief overlap is conservative and
can refuse rather than detach ownership. No unchecked Core constructor is made public.
Refutation assembly is an existing input boundary here: this change does not retrospectively
reserve earlier candidate/answer/refutation allocations or complete the case-book/RSS bound.

The actual ServiceGdnHost path uses a fallible retained emitter and the shared CourtOutbox
`try_push_clone_with_due` API. Its destination independently admits the payload copy, pending
frame and retained due-map capacity before publishing the queue entry/due date. The source action
owner remains alive throughout that handoff. `demanded`, its current submission clock, and
`accused` settlement advance only after successful destination admission; refusal leaves the
case retryable. Authenticated answers/candidate progress may remain learned across retries;
local admission failure never settles the claim as honest or manufactures a court verdict.
Legacy raw step/collection APIs remain compatibility interfaces, separate from production's
retained path. Original wire/signing domains, court verdict rules and deadline semantics remain.

## Verification status

The coordinated fresh node retry compiled successfully in 2m25s, recorded in
`/tmp/misaka-rfc-source-response-carrier-tests-retry2-20261025.log`. The build duration is not
an execution cost measurement. On the same actual node unit binary, the full
`palw_gdn_held_filer::tests` group passed **12/12** in 47.83 seconds, including both new parent
tests and the two isolated quota/lifetime children (1.95/1.81 seconds, included within the
parent counts), in `/tmp/misaka-rfc-gdn-retained-action-tests-20261025.log`.

The actual Service group `palw_panel::gdn_filer_tick::tests` passed **2/2** in 4.06 seconds,
including the new atomic handoff case and existing binding/prompt host case, in
`/tmp/misaka-rfc-gdn-service-handoff-tests-20261025.log`. The shared court-outbox group also
passed 12/12; its source-response/cancellation controls are recorded by that API's owner.
Binary: `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`.
The new cases are included in their full groups, not additional counts. No independent Cargo
job was started. Static Rust parser checks and scoped `git diff --check` also passed.

The first integrated node retry failed compilation with two E0624 errors because the action
builder attempted to call Core-private `NativeRetainedV1::from_owner_reserved_v1`
(`/tmp/misaka-rfc-source-response-carrier-tests-retry1-20261025.log`). Both paths now use the
supported public retained API while keeping their pre-construction tickets alive through that
handoff. The correction preserves output types and does not widen Core visibility. The fresh
coordinated compile/runtime retry passed as recorded above; the failed first retry is preserved
as a compile failure and not counted as runtime acceptance.

New focused filters:

- `palw_gdn_held_filer::tests::action_owner_tests`: two tests. Actual held cases reject a typed
  destination failure, retry the same signed demand/accusation, advance only after acceptance,
  and pass the final refutation through the real deterministic terminal. Two isolated children
  test zero-quota output refusal before either signer and independent demand/accusation ticket
  lifetimes. Children are included within the two parent cases.
- `palw_panel::gdn_filer_tick::tests::retained_service_gdn_demand_uses_atomic_outbox_handoff_before_advancing_book`:
  one actual Service host test checks admitted output, due date and committed demand clock.
- Existing held tick, GDN Service host and held court fixture groups remain relevant regressions.
  Shared outbox admission/lifetime tests are recorded separately by that API's owner.

These are small mechanism controls, not accepted multi-process court campaigns, all-model cost
closure, K3 inputs or production activation. Candidate/answers/evidence construction, generic
book/collection/cache memory, all other consumers and physical resource calibration remain open.
Public activation stays None, ordinary bootstrap refusal remains, and model tiers and selected
claim-verifier approval are unchanged. Local replay/court data authentication is not a
cryptographic proof of model-computation correctness.
