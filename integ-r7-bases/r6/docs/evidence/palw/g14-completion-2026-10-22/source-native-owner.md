# Native retained Source constructor — 2026-10-22

Candidate based on `fb3427f23`; release build, 21 unique parent tests and two isolated quota-worker invocations passed.

The explicit native constructor derives a checked request from actual registered program shape, StepSpace inventory, actual input/layout/context clones, RUN output and hash writer capacities, reserves before the allocating semantic constructor, then attaches the lease to the returned Source. Old call sites, source identity, schema and activation remain unchanged.

The entire conservative construction-peak ticket remains held for the Source lifetime, including temporary work that is released after construction. This is an intentional over-reservation, not an assertion of final retained size, allocator internals or measured RSS. Existing framework off-mode may return no ticket; the constructor does not claim enabled limits where disabled.

A separate process quota test checks insufficient quota before semantic construction, no partial debit on refusal, lease retention while Source lives, release on drop and semantic failure, and successful retry. Logical admission settings are global OnceLock values, so workers are isolated and ignored in normal parent selection.

The first build was deliberately aborted after read-review found a rustfmt filename heading accidentally included by the supplied test patch. [Original source/commands and aborted-build reason](source-native-owner/aborted-build-reason.md) are preserved, and the repair removes exactly that line. This is not a passing build or a claimed semantic compiler failure.

[Repaired frozen source, exact commands and executable identity](source-native-owner-repair/verification.json), [independent lifetime review](source-native-owner-repair/independent-review.md), [construction lifetime inventory](source-native-owner-repair/LIFETIMES.md). All build/native test execution uses the shared heavy lock. Full canonical graph, numerical owner, native economic profiles, actual paid ingress and atomic funded admission remain separate.

## Result

Source identity/bounds and quota parent 2, StepSpace inventory 2, retained TIR 3, t12 identity 9 and activation 5 passed (21 unique parent tests). The ignored quota worker was explicitly invoked twice in fresh child processes and both passed; it is not added twice to unique parent counts. [Exact names and summaries](source-native-owner-repair/selected-test-summary.json). Frozen source hashes remained unchanged through build and execution. This is logical allocation admission/RAII evidence, not physical RSS measurement or full native original admission.
