# Incremental review: latest Fixed numerical-root fixture revision

Read-only review of additions after INDEPENDENT-REVIEW.md. Repo/cargo/RPC untouched. No new concrete production-code correctness/ownership blocker found. All new fixture tests remain UNRUN; this memo is not a successful execution report.

## Genuine fixture construction and values
New HistoryCondition preserves old None and Input paths exactly; Fixed selects a distinct canonical registered/executed program. State declaration, Select insertion, all shifted node refs/carry_out/logits and appended writer chains occur BEFORE fixture_with encoding/class/layout/interpreter construction, then original FP-context execution/root recomputation. No admitted frame/site or asserted finalized state is edited.

Per-layer scalar Fixed state has I8 [0,1] bounds containing normative initial0. Each distinct scheduled layer block gets one StateWrite for its own scheduled instance. Native state_occurrence/state_writer then resolve the same layer instance. The actual CarryIn vector is rank-reduced by repeated legal Gather axis0 with real scalar Idx0; each output drops the current first dimension and retains carry dtype. Clamp then returns scalar I8 [0,1]. Existing Clamp validator DOES allow this bounded output dtype change (unlike an implicit cast assumption); StateWrite output/state shape+dtype match. All nodes are live via real StateWrite effect. No fabricated raw values or model parameters are introduced.

Both Select value branches are the same actual previous operand, so choosing condition0/1 cannot change that existing numeric branch result. State itself is still honestly computed from real carry, not a fake constant supplied at roots. Committed mode adds an actual committed writer leaf. Noncommitted mode recomputes real Gather/Clamp/StateWrite from authenticated carry. Original artifact/execution roots may change due program/leaf geometry and are recomputed normally.

## Exact boundary timing
At original position0, state is normative zero and requires no external state body. Positive checkpoint reads actual State leaf at pos-1. Positive noncheckpoint reads writer output at pos-1; committed mode therefore requires exact new StateWrite node/occurrence at prior position. Noncommit mode follows same typed Walk under prior context and ultimately requires actual previous-occurrence CarryIn commit at prior position. Current committed writer is appended after layer nodes, but prior position still strictly precedes any current root, so leaf-order requirement is consistent.

The fixture and test do not reinterpret positive checkpoint as automatically zero or independently infer release/financial state. Source authentication of actual dependency leaves establishes data identity, not correctness of arbitrary computation.

## Legacy relation / counters
For a committed writer the old parameter Walk did not recurse into writer; tracked path now records its leaf with no extra edge/node. For a noncommitted writer both old None and tracked paths set the same pos-1 writer occurrence, enqueue the same node and restore context; Gather/Clamp ultimately fetch CarryIn, whose recording adds no edge beyond its already-charged fetch. Hence exact parameter relation and exploration counters should remain unchanged for each new program, as asserted. Program changes legitimately alter the fixture's geometry; assertion compares old/new algorithms on SAME new original, not old original bytes against new original.

No production allocation quote change is needed for test-only program growth: actual native prelude scanner/StepSpace/inventory quotes run against the enlarged real program. Boundary type/edge limits are still actual typed quotes, not fixed legacy sizeof assumptions. No new caps/prices/completeness flag.

## Test strength and remaining limits
The test requires nonzero new-state checkpoint boundaries and positive prior-writer dependencies for BOTH modes, authenticates actual execution StepLeaf openings, and compares entire old parameter relation/counters. Committed-mode exact newly introduced writer node is a strong causal witness.

Noncommit-mode prior_writers count matches a CarryStepLeaf at the expected prior writer context/position. This confirms the named data coordinate exists, but does not strictly exclude coincidental same carry dependency from another existing history path. A future counterfactual baseline (same geometry without State condition) or observed writer-demand trace would make that causal assertion stronger. This is a test-specific robustness limitation, not a found production mapping bug. The actual code directly follows state_writer and typed Walk and was reviewed separately.

The test authenticates bytes and fixtures are real interpreter executions; it does not newly assert state scalar values vary across positions, compare per-position Fixed values against an independent formula, or prove all Fixed grammar cases. Unwritten-state checkpoint/zero fixture and malformed boundary negatives remain absent. No full semantic/source/financial/G14 completion follows from this added coverage. Existing native/golden/corpus regressions are still required for shared helper extraction.

Parent subsequently reported actual build type errors confined to the new Fixed test: Commit.occurrence is u32 whereas Demand occurrence is u16, and usize::from(u32) is unavailable. Those type mismatches were not caught by this read-only review. Parent's planned checked usize::try_from and lossless u32::from(writer_occ/prior.occurrence) fixes preserve test meaning; they do not relax native validation or change production behavior. Failure source/log must remain saved before repair.
