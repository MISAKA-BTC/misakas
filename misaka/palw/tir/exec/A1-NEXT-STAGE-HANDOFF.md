# A1 next stage: pure expanded plan and primitive-record delivery

2026-10-31. Source commits: **`05c47201d` expanded geometry**, **`8e3c2e481`
primitive-record adapter**. Shared record codec is A0 `d284f5902`, imported as
`e03a09427`. The import adds only the provided record module/codec/tests; it does
not import unrelated native/rows/funding implementations from A0's later tree.

## Pure expanded plan: implemented, not an active derivation

`node::positive_check::expanded_plan` defines independent `ExpandedPlan`,
`ExpandedPlanLimits`, `ExpandedSummary` and relation DTOs. It does not weaken or
route through the existing all-node-commit `CoveragePlan`.

```rust,ignore
let plan = ExpandedPlan::derive(&registered_class, artifact, &native_job, limits)?;
let summary = plan.summary();
let relation = plan.relation_at(index); // or plan.iter()
```

The original program/commit flags, dtypes, layout, job context and legacy leaf
numbers remain unchanged. Original uncommitted i64/i128 nodes are included in
position/occurrence/node/lane order. Every output lane is covered, including
lanes unused by a later slice. Existing commit tiles, Fixed checkpoints and Hist
tiles retain their legacy identity/order. Prefill post skipping, repeated layer
blocks and `H=min(position+1, window)` use original TIR execution geometry.

`legacy_frontier` is the required geometric prefix boundary, **not a claim that
anything was checked**. The plan has no Deserialize or arithmetic-success/native
admission constructor. Its public relation DTO is not an authorization token.

The planner bounds all structural visits before position loops. It stores only
per-position prefixes, rather than all relations. Checked prefix metadata bytes,
all target lanes, maximum `Vec<i128>` result storage (16 bytes/lane), legacy and
expanded relation counts, evaluation reservations and every aggregate work sum
are capped. Allocation uses `try_reserve_exact` and verifies actual capacity.
The diagnostic `structural_work` sums existing `node_cost` once per executed
node; it is neither reward accounting nor the exact cost of repeatedly evaluating
fresh cones. Repeated lookup and future source/evaluator/RSS costs remain separate.

This phase only derives geometry/work. It neither evaluates original wide
intermediates nor accepts material for them. No expanded derivation ID,
source-only payload, wire tag, native receipt or public activation is assigned.
The initial checker still rejects sparse wide programs; that boundary is tested.
Wide logits require the separate RFC-0043/ADR-0211 integration, absent and inactive here.

The concrete source-factory preparation is in
[A1-AUTH-SOURCE-FACTORY-DELTA.md](A1-AUTH-SOURCE-FACTORY-DELTA.md). It identifies
existing private helper extraction points while preserving old Court fault order
and the one-way core dependency. Core opaque units authenticate membership and
geometry; Node-owned private checked-prefix state supplies computation authority.
No arbitrary callback or authentication boolean is proposed as an execution source.
That factory is still unimplemented and waits for A0's formal source-only annex.

## Primitive-record ingress: implemented for the existing initial profile

API in `node::positive_contract::record`:

```rust,ignore
let context = prepared.authenticate_record(
    witness, record_bytes,
    RecordLimits { max_record_bytes, max_generated_heap_bytes },
    &panel_key, &panel_witness_signature,
)?;
// A bounded u64 cap is also accepted for A3's existing wrapper.
```

The receiver borrows `PreparedCheck` and accepts no peer Result/class/job.
`ExecutionRecord::decode` uses the preparation's signed fixed decode count;
`validate_binding` compares its stored Result and the supplied signed Witness.
Generated count*4 is checked before `try_reserve_exact`/collection, with a second
capacity check. Binding roots come from the primitive record; class/job and
artifact identity come from the retained, independently authenticated native
preparation. No class/program/job is reserialized into a record carrier.

The reconstructed binding goes through existing `authenticate_witness` for
signature, plan, native/CU/tokenizer/context, provenance and candidate-output
checks. All errors are `RecordDeliveryError`: delivery refusals only, never
WitnessRejected, Miner refutation or attempt consumption. A bad delivery cannot
mutate prior progress, and the same borrowed preparation can accept a correct
retry. Native attempt/funding/duty state is not represented by caller booleans.

`max_generated_heap_bytes` bounds only the new logical `Vec<u32>` backing storage.
Retained native metadata/plan copies, authentication and evaluator workspace need
separate native reservation. This helper does not certify total Node RSS or
native acceptance. The u64 convenience cap sets both local record and generated
limits; native callers may pass explicit independent limits.

After this delivery stage, use `CheckContext::start` and the existing
`TirNodeChecker::check_signed_part` or authenticated-frame helper. A record alone
never advances arithmetic coverage. The record path remains on the old initial
all-node-commit/i32 profile and does not enable expanded/wide verification.

## Validation and handoff

Independent expanded fixtures cover original sparse i64/i128 hidden lanes,
dynamic H crossing W-1/W/W+1, repeated layer occurrences, Fixed/Hist, legacy
frontier identity, caps, unchanged input registration, and old-profile rejection.
The MatMul/reduction fixture independently derives EW20, MAC12, no transcendental
operations, 110 read bytes and 90 written bytes for P2/D2. A unit test checks
arithmetic/structural overflow rejection.

Record tests use the actual common codec, Result/Witness signatures, retained
native context, full signed parts and Node terminal comparison. Eleven corrupt
delivery variants, logical generated-buffer caps, bad keys/signatures and wrong
candidate output refuse only delivery; valid retry and prior progress still finish.

Raw logs, candidate hashes and machine-readable totals are recorded under
`validation/next-stage/`. The earlier 52-test initial-profile evidence remains
unchanged. No A4 native/UTXO/funding behavior or public activation is claimed.

Final source candidate `8e3c2e481`: 36 integration tests (existing Node 28,
expanded 4, record 4), one overflow unit test, and six shared record-codec tests
pass: **43 tests, zero failures**. Package-only Clippy passes with `--no-deps
-- -D warnings`; dependency-inclusive baseline findings remain in the earlier
handoff. Docs are checked in no-new-regressions mode, not a clean strict audit.
