# RFC-0006 retained held-GDN answer inputs — 2026-10-25

## Actual answer consumer

The signed-action/output handoff already has independent retained ownership. Before it, the
actual held filer copied an authenticated producer disclosure into raw `case.answers`, then
cloned the complete answer set again before refutation assembly. Neither the persistent chunk/
Merkle-path copies nor the simultaneous second copy had before-allocation admission or a
retained lifetime owner. This continuation repairs those actual input consumers, without
claiming completion of the entire case/candidate/refutation graph.

Case answers now hold immutable `NativeRetainedV1<PalwHeldDisclosureV1>` values. The reader first
selects an actually matching disclosure and runs the existing authentication, then reserves its
actual typed deep copy before cloning any bytes/path. Resource refusal propagates separately
from absence/invalid membership: the actual retained step returns Resource before it can demand
again, declare a timeout/default, sign, or infer an honest result. Matching replay and byte
membership remain distinct from claim approval.

The persistent Option slots are reserved from the actual new demand count and actual retained
slot type before allocation. Candidate loading constructs these new slots before committing the
candidate, demands, answers and demand clock; refusal preserves the preceding candidate's
answers. The slot owner lives in the case, while each disclosure has an independent ticket.
Derived `Clone` on the case is removed so ordinary clone syntax cannot silently deep-copy
retained inputs. The historical open/step interfaces preserve their error/output compatibility;
returned cases still retain their answer owners.

Before the complete-answer copy, a scoped batch owner derives destination slots and every
actual source disclosure's heap. Admission precedes all clones; the batch retains its own ticket
through refutation assembly, the court terminal and signed-output construction. Case answers
remain independently owned across retries and after the temporary batch/action drops. No wire,
row, policy, public activation or court-selection rule changes.

Authentication scratch, initial recomputation, candidate enumeration/demand construction,
refutation construction itself, generic case-book/map/host-history allocation and physical
allocator/RSS bounds remain separate open work. Counts here come from actual typed inputs and
native heap visitors, not a byte-to-RAM ratio or invented fixed prefix bound. The existing
material/capture and signed-action owners do not retroactively cover these other allocations.

## Verification status

The coordinated actual node build passed in 1m39s, recorded in
`/tmp/misaka-rfc-fp-routed-replay-tests-20261025.log`. The full held-filer group passed **15/15**
in 54.07 seconds in `/tmp/misaka-rfc-gdn-answer-owner-tests-20261025.log`, including all three
new answer-owner parent cases. Their isolated children passed in 1.81 and 1.41 seconds and are
included within those parent counts. Two older signed-action children (2.37/1.90 seconds) are
also included within their existing parent case; they are not new answer-owner tests or extra
distinct cases. The build duration is not a model execution cost measurement.

Actual binary:
`/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`, SHA-256
`a0410f613b5236edfc3058a922699d065e3bda9a7f03243398bb42a97bb80ba9`.
This records the tested production source generation; a later unrelated test-only source-pin
correction/rebuild does not retroactively change that artifact identity. No independent Cargo
invocation was started. Static parser and scoped `git diff --check` controls also passed.

New filter `palw_gdn_held_filer::tests::answer_owner_tests` contains three parent tests. Real tiny
producer disclosures test foreign/invalid membership rejection and authenticated answer
retention. An isolated actual-step test exhausts admitted quota while a demand is long past its
old deadline, observes Resource with no signature/default or cursor/clock change, releases the
blocking loan, retries to a real court accusation, and checks persistent-answer ownership until
case retirement. A second isolated test checks full-answer-batch and Option-slot refusal before
copy/allocation, source preservation, successful retry and independent owner lifetime. Children
are included within the parent test count. Existing held-filer, Service host and held-court
fixture groups remain relevant regressions.

These are small mechanism controls, not K3, all-model cost or a multi-process court campaign.
Formal K3 specification/manifest/weights/signed inputs remain uncreated and the fifteen campaign
gates remain UNKNOWN. Activation stays None, ordinary selected-source/V6 refusals and model
tiers remain; claim-level selected-verifier authentication is unchanged.
