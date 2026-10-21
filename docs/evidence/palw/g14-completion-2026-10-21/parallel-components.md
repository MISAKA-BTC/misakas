# Parallel component implementation — 2026-10-21

Status: initial combined component build and 9 tests pass; **not G14 completion**. This record does not grant default permission or financial release.

## Causal reservation

The planner reserves every supplied graph node after all predecessors, atomically, using canonical ready ordering and independent byte/work/IO/control capacity. Original packet promises and existing slots are retained. A repair proposal that moves a missing parent after a retained child is rejected without changing the book. Conservative first-fit may reject a feasible graph; it is not a complete feasibility solver. Native original binding must retain/rederive the complete graph and check it on every load and mutation. Generic v2 repair alone is insufficient. Actual parent completion and response eligibility are separate from reserved slots.

Initial four-test source, raw log and hash-pinned executable remain in the causal-service records. The later recovery/capacity changes were built and tested separately below; do not attribute the initial four passes to those changes.

## Potential parameter dependency cover

The isolated read-only derivation uses an original native binding/site/inventory and explores possible parameter coordinates. Unknown Gather indices cover their finite axis; Select covers both possible branches. Structural/arithmetical coordinates, per-layer coordinates and Fixed-state replay back to a checkpoint are followed. It does not rely on one honest execution's read set or require a withheld producer root before opening.

The returned finite envelope is not an exact canonical carriage, an authenticated original contract, funded acquisition or an admission certificate. Explicit traversal limits fail without returning a partial cover. Existing binding decoding, range analysis, inventory construction and scratch allocations require separate native preflight accounting. The current sampled builder comparison and a Gather/Select read-set counterexample do not prove exhaustive adversarial root/bottom coverage.

## Archive event inspection

A private token inspects the native wrapper's actual current event, original signed receipt, original window and state root. No caller timestamp, paid flag or peer-deserialized certificate establishes timely completion. Boundary and tamper checks extend the existing signed native-v5 archive fixture.

Inspection alone changes neither archive fulfillment nor money. The user selected S1 for detailed isolated implementation in RFC-0034/ADR-0201; the financial writer is not yet implemented. Public archive intake, actual fees, same-root v5 completion, once-only financial effect, other liabilities and pruning remain required; no public entry or bond release is enabled by this component.

## Build and checks

`misaka-heavy-lock.sh cargo test --release -p kaspa-consensus-core --lib --test palw_tir_parameter_cover --no-run` completed successfully. Both test executables were copied with executable modes retained and SHA-256 pinned before any later source edits. [Build/source metadata](parallel-components-build.json), [raw build log](parallel-components-build.log).

The pinned executables ran sequentially under the heavy lock:

- [Causal reservation](parallel-causal-tests.log): **6 passed / 0 failed**, including aggregate capacity rollback and unsafe-parent-repair refusal.
- [Archive inspection](parallel-archive-tests.log): **1 passed / 0 failed**, extending the existing native-v5 fixture's 8 signed cases; those internal cases are not 8 additional test-runner passes.
- [Parameter cover](parallel-parameter-tests.log): **2 passed / 0 failed** (sampled existing finalize inclusion and the Gather/Select single-execution counterexample).

Fresh non-test-core-linked [t12 identity and activation checks](parallel-identities.log): **9 + 5 passed**, unchanged dormant release fingerprints/fences. These integration executables are preserved in the same build metadata.

No new performance, real-node, real-model door or saturation measurements are claimed. Exhaustive adversarial cover and S1 financial integration remain unimplemented/unverified here.

## Original admission integration audit

Read-only inspection identified the following concrete boundaries for the next integration:

- Existing TFP1 accepts only its pinned version/shape and full input consumption (`palw_tir_public_fp_envelope_v1`). A new optional tail cannot silently add shared obligations. A separate candidate envelope/signature domain is required.
- `palw_tir_public_fold_v1::prepare_original_v1` and `install_original_v1` share the native FP atomic checkpoint through `palw_state_v2/objects.rs`. New pairing detection must participate in that checkpoint before ordinary FP writes; otherwise a failed new allocation could leave the FP claim behind.
- The old producer custody pledge signs its old resource-plan/source/scope and horizons. It does not consent to a new whole-graph fee/capital contract, nor does the producer authenticate future Panel/relay obligations on their behalf.
- The actual carrier fee is checked in `palw_accept.rs`; the declared original rent is removed from collectible fee and burned in `consensus/src/pipeline/virtual_processor/utxo_validation.rs`. Burn is not future Panel or relay funding. Frame deposit admission checks native bond room and retains nominal exposure; it is not prefunded escrow. Existing slash/burn writers must not be relabeled a new service payment source.
- Accepted DAA is not known at ordinary pre-signing. A future consent must bind a canonical relative plan plus an explicit acceptance interval/horizon (or a deliberately exact target DAA), and derive absolute deadlines at actual acceptance. This is an unresolved contract design requirement, not an adopted alternative.

The graph cannot be authorized only by non-rooted transition extras: load, repair, forward mutation and reorg must retain its original binding. The current causal scheduling helper and v2 book do not supply this missing contract or actual funding.

## Related regressions and document check

The same preserved core executable also passed [41 native frame tests](parallel-frame-regressions.log) (including the one archive inspection test above, not an additional pass) and [8 D2 route/root tests](parallel-d2-regressions.log).

`generate_indexes.py` was run. [Document checker](parallel-docs-check.log) still exits 1 for the same 5 imported missing references and 43 known baseline occurrences. No new authored missing reference or stale index remains; the baseline and imported raw assets were not rewritten.
