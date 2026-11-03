# A7 class-only analysis harness

Source base: Root `07d7eaebad0021164476c6289d8ccbbcbc45862c`.
[real_llm_class_analysis.rs](src/bin/real_llm_class_analysis.rs) implements the
[reviewed plan](evidence/pure-admission-plan-cc22-2026-11-01/PLAN.md) as a
separate binary. At source freeze it has not been typechecked, built or run.
A7 Cargo and large runtimes remain on hold while A6 owns the heavy build slot.

This is metadata, geometry and close-size analysis of the original three
trained-artifact declarations. It does not execute model inference, construct
a registration, sign, create collateral or origin, install a class row, enable
Params, or grant native authority or credit. Missing authentic registration
fields keep constructor and full pure admission at
`NOT_RUN_MISSING_REGISTRATION_INPUTS`.

The CLI requires every argument explicitly:

```text
real_llm_class_analysis
  --model HuggingFaceTB/SmolLM2-360M|HuggingFaceTB/SmolLM2-135M|EleutherAI/pythia-70m
  --artifact EXISTING_DECLARED_ARTIFACT
  --fixture ORIGINAL_REGISTRATION_FIXTURE
  --diagnostic-leaf-cap EXPLICIT_POSITIVE_U64
  --diagnostic-court on|off|both
  --out NEW_REPORT_PATH
```

No target/cache/binary is selected automatically. The output file is created
exclusively. A compile and separately authorized process timeout/RSS wrapper
are required before execution. The existing CPU process driver's fixed argv
and success schema are different; it must not be pointed at this binary as
though it were a CPU/capture run.

## Identity and contexts

The fixture SHA is checked before JSON decoding; model selection pins the
original fixture/artifact SHA and revision. The complete artifact is streamed
for SHA identity, then its header is read through the opt-in bounded header
reader. Program/layout/tokenizer bytes are compared exactly with the fixed
fixture before canonical and StepSpace analysis. There is no model fallback,
conversion or context reduction.

Artifact SHA identity is distinct from recomputing a tensor inventory root.
This analysis uses the original fixture's declared root to recompute class
identity, and records `inventory_root_recomputed=false`. It does not inherit
native authority from a matching digest or from the earlier CPU run.

| Model | Original layout maximum positions | Canonical P,D | Canonical positions | Canonical token budget |
|---|---:|---|---:|---:|
| Smol360 / Smol135 | 8192 | 1023,2 | 1024 | 8193 |
| Pythia70 wide | 2048 | 255,2 | 256 | 2049 |

These are the existing canonical formulas: positions count `P+D-1`, while
token budget counts `P+D`. The preserved canonical is exported separately
from the longest analysis probe `P=1, D=original maximum positions,
max_context_tokens=u32::MAX`. That internal sizing probe is not a new job
permission or a CPU execution. Neither context is the short P16/D1 CPU job.

The explicit diagnostic leaf cap is not the protocol ladder. Protocol ladder
fitness remains NOT_CHECKED without the actual bundle/ladder.

## Sizing and output

Smol uses V1 narrow element/range APIs. Pythia uses explicit typed V2 sizing,
which conservatively prices all step lanes at 8B and leaves the actual mixed
wire unchanged. Every case uses Multiproof, fixed work cap `1<<26` and
`stop_above=None`. `on`, `off` and `both` select clearly labelled diagnostic
court assumptions; no protocol rule or fee is chosen on their behalf.

A completed sizing must return exactly the ordinary committed `(block,node)`
set, without duplicates or GEN checkpoints. Layer occurrences are not added
as extra commit keys. Element/range bounds are compared only when both finish
under the same court assumption; work counters remain separate. A refused
count or failed/over-cap/incomparable sizing cannot make the whole analysis
complete. Partial API work is reported as unknown, never `cap+1` measured work.

Each case separately records terminal close maximum, dissected root-claim
maximum, the fixed one-carrier root format bound and all point bounds. Coarse
close bytes and the combined `max(coarse,typed_max)` envelope remain
NOT_CHECKED without admission model rules, demand/F7/window and ceilings.
Actual terminal court/carrier/chunk-policy checks are also NOT_CHECKED. A
full data traversal is distinct from a root/carrier fit or pure admission.

The output schema is `misaka.a7.real-llm-class-analysis.v1`, scope
`CLASS_ONLY_DATA_ANALYSIS`. Exit 0 means the requested diagnostic counts,
full traversals and same-court twin comparisons completed; it does not mean
the class was admitted. Exit 2 preserves incomplete case results; argument,
identity or analysis errors return a nonzero result with phase/error details.

## Resource boundary

Header reader limits describe that parser's local backing only. Other fixture
owners, program/layout decode, StepSpace/inventory, read maps, sizing caches,
bounds and JSON output are separate owners. The sizing work cap and metadata
cardinality guards are not a whole-process RAM/work lease, measured RSS or a
fee guarantee. Startup allocation-before-limit safety remains unestablished.
No model/registration/credit result is inferred from these resource checks.
