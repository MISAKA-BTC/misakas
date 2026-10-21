# Independent read review — numerical material join

Reviewed module.rs, tests.rs and NOTES.md against the existing SourceGraph request geometry, opaque OriginalRootInputCover, native numerical geometry and Store's guarded terminal implementation. Repo/build untouched. No concrete blocking issue found by read review. New code/tests remain uncompiled/unexecuted.

## Actual binding and inventory

- Native geometry and opaque cover must match source ID, original leaf count, root count and exact per-root leaf relation. Parameters are read from the opaque cover's immutable native-produced relation, not an arbitrary caller DTO accepted as complete.
- Geometry arity/demand are checked against explicit configuration arguments; opaque SourceGraph is checked against actual frame/source/profile. This is consistency, not authentication of those passed configuration bodies. As NOTES states, the actual coordinator must resolve authoritative native configuration; this helper must not promote its demand/court arguments to native policy proof.
- Every root itself and its actual Carry/Committed/FixedCheckpoint/History source boundary joins the exact StepLeaf query Finish for every native supplier role. GeneratedId input joins FullIds Finish; prompt and parameter remain exact typed unresolved coordinates. The loop checks role numbering, not cardinality alone, and absolute IDs come from the same allocation scope.
- Duplicate coordinates under different roots/boundary relations conservatively retain separate memberships and are counted before Vec allocation. No caller subset, arbitrary successful closure flag, invented parameter query or new supplier pledge is accepted.
- The numerical path owner remains borrowed so all actual paths use the same root dependency relation without duplicating path Vecs.

## Runtime and current state

- check_material validates the Store source/header and reuses existing authenticated all-root input/parameter guards, then explicitly requires the authenticated numerical-root StepLeaf itself. This checks held bytes only, not availability/retention/payment.
- build_terminal checks current frame/graph, actual court arity and ladder, selects an actual numerical root and native terminal path, then calls the existing parameter-guarded finalizer. The latter still verifies current active actor signature, scope/policy/root statement, actual root/ancestors and timing, native claim binding, canonical output and bottom cost. No cached outcome or metadata helper bypasses those checks.
- The builder wrapper has no new current-registration-demand resolver. Its passed geometry's demand remains caller-native provenance, as already documented. A later coordinator must match actual registered/native demand before treating this metadata as an authoritative admission inventory. This helper provides no such admission authority today.

## Ownership and tests

- Metadata count/checked multiplication and traversal bounds precede output Vec allocation. Retained output includes its single exact dependency Vec plus conservative frame clone/hash scratch. Local visit exhaustion is typed Resource and drops candidates; it does not classify a claim as fraudulent.
- Actual Source/geometry/cover/graph owners are borrowed, keeping their tickets alive. Builder Source reconstruction, signatures, proof/finalizer cloning and other transients remain separately admitted. The metadata quote does not claim to cover these separate operations.
- Fixtures cover actual Input0/history with Flat/Merkle prompt and actual Fixed committed/noncheckpoint input boundaries; actual authenticated StepLeaf/parameter/FullIds material is used. Wrong profile, foreign opaque cover, missing source units/prompt/parameters and local quota−1/hold/drop checks are meaningful. The sourcegraph quota derives from actual transport metadata and does not change protocol caps.
- New tests validate the joined readset and supplier mapping. They do NOT execute this new wrapper through a complete signed ancestor/finalizer positive; existing finalizer tests cover the delegated implementation separately. NOTES accurately discloses this limitation rather than claiming new end-to-end completion.

No full-original closure, adopted profile body, funded/paid status, network supply result or financial installation is established by this candidate.

## Open issue found in parent review after the initial read

Parent identified a concrete blocker missed in the initial review: `build_terminal_v1` invokes `check_material_v1`, which requires every root's inputs, parameters and root StepLeaf. That confuses reservation across all potential roots with runtime material for the selected root/path. A legitimate local terminal must not require unrelated roots' actual bytes merely because they were included in the original potential allocation inventory.

Fix required: retain an explicit whole-inventory material-audit API if useful, but make runtime finalization call a selected-root guard that checks only the chosen root's actual dependency/parameter/rootStepLeaf relation. It must select the root from the immutable native cover/geometry, not accept an arbitrary partial caller DTO, and preserve current signed root/ancestor/native finalizer checks. Add actual fixture coverage where unrelated root material is absent yet the selected-root guard succeeds, while selected-root missing input/parameter/root body still refuses. Revised candidate pending; initial 'no blocker found' statement above is superseded by this open issue.

## Selected-root correction re-review outcome

The unrelated-root runtime issue is resolved by the latest `/tmp/g14-numerical-selected-root/numerical-selected-root.patch` by read review. Detailed independent review is saved in that directory's INDEPENDENT-REVIEW.md. Runtime now calls an opaque named-root guard while the global audit remains separately O(total-inputs); native signed ancestor/finalizer positive compares unchanged proof bytes/meter with unrelated roots absent. Selected-root remains explicitly conservative, not selected-path exact-minimal. All runtime/compile tests are still unexecuted at this review.
