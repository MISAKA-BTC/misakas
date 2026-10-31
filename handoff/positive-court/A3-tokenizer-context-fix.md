# A3 — preserve canonical job tokenizer independently of registration

Source `86932f2c7` fixes the tokenizer conflation found in the independent A0
review of `94e18252f`. Registration metadata may contain tokenizer 18 while the
actual canonical TIR job contains tokenizer 0. The registration's class ID pins
the former; the exact expected job/context and Result pin the latter. Neither
field is copied into the other.

The old source/evidence `94e18252f` / `211d70b6b` and its 62-test report remain
unchanged history. Its fixture had overwritten the job tokenizer with the
registered value, so those results are not evidence for the canonical-zero case.
The corrected fixtures retain `node_common::job`'s actual zero marker and the
nonzero registered tokenizer independently. Expected native context hashes are
fixed before attempted substitutions; old rewritten-fixture hashes are not used
as expected roots for the canonical job.

Removed only the erroneous class↔job/Result tokenizer equalities in typed
capture, Panel admission, the downstream core CoveragePlan and Panel cold-source
cache. The actual class ID/artifact/program/layout checks still authenticate all
registration metadata. `result.tokenizer == job.tokenizer`, rooted Result/job/
execution/input bindings, seed and counts remain required. The existing r1
generator APIs, original program/layout/dtypes/maxctx, old leaf scheme and Miner
signing obligations are unchanged.

The local typed prototype additionally takes an explicit independent context pin:

```text
quote_typed_prototype(runner, registered, artifact, job, expected_execution, limits)
capture_typed_prototype(runner, registered, artifact, job, expected_execution, prompt, limits)
```

`expected_execution` comes from the independently resolved expected native job,
not a context hash computed from a delivered candidate. The full literal job
hash must match, including tokenizer, input, seed, shape and other context fields.
Cheap input/format checks and hash scratch reservation still precede hashing.
No tokenizer constant is forced: zero is accepted when it is the actual expected
value; other independently expected literal contexts are not rewritten either.

On the exact source, **63 selected tests passed**: A1/A3 execution and typed
capture 48, original-author signed Panel/record/cold-source transport 15.
Positive cases exercise canonical tokenizer 0 with registered tokenizer 18,
ordinary runner versus one Panel replay with identical roots, preserved full
binding context, all original mandatory frames and actual A1 terminal results.
The cold A2 fallback reconstructs the registered artifact and prompt and reaches
the same actual checker without changing the expected job.

Negative cases cover a changed registration tokenizer with fixed class ID,
changed job tokenizer/input/seed/shape/context with a fixed expected hash, a
coherent foreign input/seed/tokenizer packet, and a genuinely re-signed Miner
Result copying registration tokenizer 18 into the actual job's zero field.
Returning to the original input still succeeds. A wrong registration cannot be
made valid by setting its tokenizer to the job's zero marker.

Strict combined exec/remote Clippy stopped on two unchanged A2 recovery lints:
`misnamed_getters` at `positive_miner_v2_recovery.rs:424` and `collapsible_if`
at line 442. The recovery source is byte-identical to parent `211d70b6b`; raw
diagnostics are retained. The intended `anchor()` return of `committed_anchor`
was not changed to satisfy a naming heuristic. The separate exec strict check passed with zero warnings. Documentation
no-new-regressions passed with 43 known issues and zero new occurrences; the
final staged-document check is recorded in the validation metadata. This
increment does not claim a clean strict remote Clippy run.

[Exact commands, source/tree/binary hashes and raw logs](A3-r1-evidence/tokenizer-context-fix-94e18252f/validation.json)
keep these results separate from historical counts and from the next adapter.
At this tested source the A1 ExpandedPlan same-equality was an owner follow-up;
it is not used by these capture/r1 paths. A0 subsequently selected the separate
A1 correction as `4bf984f35`; that later integration is not counted here. Latest
A2 inference/native integration fixes were not bulk-imported. No native
admission, new public commitment/ID, real funding/locks, Gate C or activation
success is inferred from these role tests.
