# RFC-0029 native Preparation / Acquisition continuation retained Source

Status: integrated no-run build PASS; new focused runtime PASS (3/3); related regressions PASS. This connects existing native control consumers, not a new policy, installer, activation or authority.

## Actual production connection

The real `apply_control_v1 -> apply_verified_control_inner_v1` Preparation and Acquisition branches previously reconstructed Source through raw `public_state` transition functions and mapped their String results to semantic refusal. Their Source construction did not use native admission. They now call additive crate-local native transition entrypoints. Source construction Resource stays typed and maps to `PalwStateV2Error::NativeResource`; action/identity Decode keeps the existing semantic refusal and ordinary checkpoint rollback.

The old public raw/String transition and source-row APIs still use raw `Source::new_v1`. They are not native-to-String compatibility wrappers. Shared private generic row drivers preserve the same original row/frame/bundle/class/layout/leaf/source-ID checks and order, with only constructor strategy differing. Frame's additive `source_identity_native_v2` shares the old descriptor checks and uses retained construction; it does not introduce a full-frame revalidation on the per-action identity path.

The native source-row helper returns its existing `NativeRetainedV1<Source>` to the native transition. That owner lives through the shared existing action implementation and row structure checks. Preparation capture/Ready/collect and Acquisition demand/answer/debit actions use the same action functions; no local data becomes selected approval. The owner drops on return/refusal after its last borrowed action/row check. The dormant bundle branch keeps all old reconstruction/work checks, using the retained constructor only when the native entrypoint is chosen.

## Executed actual-consumer controls

Filter `continuation_source_owned`: three outer tests; each has one isolated child included in its parent's count.

- Real ML-DSA producer Capture and PreparationReady are separately established as positive controls before quota. Under quota armed before TransitionBuilder construction, each must refuse at the actual Source constructor (one attempt, no constructed owner), with outer NativeResource and unchanged state root/journal. Ample same signed input retries.
- Real owner Open follows actual producer Ready, then a signed StepLeaf Demand exercises the actual Acquisition branch. The same Source-stage quota / rollback / retry controls apply. Earlier signature/builder/checkpoint refusal cannot substitute for the required Source attempt.
- Direct typed transitions for all three actions use actual quoted Source capacity: q-1 must refuse at Source; exact q succeeds, and a read-only observer after action/row validation tries a real extra byte reservation and requires refusal while the actual owner is still borrowed. Full q is reclaimable on return. Wrong expected source is semantic and releases the owner; original input retries. Observers count/observe only and cannot supply authentication, accepted material or injected Resource.

The existing real-key source-three fixture binds key before job/context/claim derivation and uses actual frame/signature constructors, native panel duty writer, capture/Ready/Open controls. Its accepted original claim and drawn panel are explicit fixture assumptions; these tests are not a mined paid-admission or selected-approval campaign.

## Remaining scope

Only the actual Core Preparation and Acquisition fold callers switch to these native entrypoints. Node owner/custodian readers and other public snapshot/shared-material raw callers remain separate; no missing shared-context installer is fabricated. Next-row / frame / transition Vec cloning, dormant original-job reconstruction, raw action-internal cost taxonomy and serializer/runtime allocation are separate boundaries, not covered by the Source ticket. Existing TransitionBuilder state/checkpoint/journal reservations are not duplicated.

Wire/schema/source identity, class/model tiers, normal bootstrap/arming refusal, public activation None, K3 absence and campaign UNKNOWN remain unchanged.

## Focused runtime evidence

Integrated build passed in 5m33s with all 15 recorded source identities matching. Immutable Core binary: `/tmp/misaka-rfc-view-continuation-core-20261025`, SHA256 `75c576318084b4435dd6fff127b90aa8cf3a468bdc856b0d4c7ee6f98c9ac7c4`.

`continuation_source_owned`: **3/3 PASS**, 1.42s test harness / 1.43s wall, log `/tmp/misaka-rfc-view-continuation-continuation_source_owned-tests-20261025.log`. Isolated children are included in these three parent tests. This validates the actual signed Capture/Ready/Demand native Source stage, typed quota refusal / rollback / same-input retry, direct final-borrow quota pressure and semantic release controls described above.

The seven related regression groups completed successfully on the same immutable Core binary, as listed below. Independent Phase1/as-of fixture failures are outside this source consumer scope and are not counted toward its result. Rust is frozen; no further Source implementation change follows these results.

Same immutable Core regression results (children included in parent counts):

- Derivation **1/1 PASS**, .12s: `/tmp/misaka-rfc-view-continuation-derive-regression-20261025.log`.
- Preparation **2/2 PASS**, .77s: `/tmp/misaka-rfc-view-continuation-preparation-regression-20261025.log`.
- Header copy **2/2 PASS**, .45s: `/tmp/misaka-rfc-view-continuation-header-regression-20261025.log`.
- Source3 **4/4 PASS**, 1.83s: `/tmp/misaka-rfc-view-continuation-source3-regression-20261025.log`.
- Output **2/2 PASS**, .22s: `/tmp/misaka-rfc-view-continuation-output-regression-20261025.log`.
- Funded candidate **1/1 PASS**, 2.50s: `/tmp/misaka-rfc-view-continuation-funded-regression-20261025.log`.
- Allocation **8/8 PASS**, .91s: `/tmp/misaka-rfc-view-continuation-allocation-regression-20261025.log`.

New focused test children took .23s (direct lifetime/semantic release), .46s (signed owner Demand), and .71s (signed Capture/Ready); these are included in the three parent results. No remaining runtime item for this owned five-file change is labelled PENDING. Node raw consumers and separate resource boundaries remain explicitly unfinished above.
