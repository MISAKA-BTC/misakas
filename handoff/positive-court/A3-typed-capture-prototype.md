# A3 — source-only full typed capture prototype

This is a local Panel data-capture experiment on A0 checkpoint `f6c7d9f4d`.
It leaves the registered program, sparse commit flags, layout, dtypes, maximum
context, old step numbering and r1 APIs unchanged. The Miner generates no extra
record and signs nothing again. There is no native admission or computation
credit capability in the returned type.

## API and one-pass execution

`node::positive_typed_capture::capture_typed_prototype(runner, registered,
artifact, job, prompt, limits)` returns an immutable `PrototypeTypedCapture`.
`quote_typed_prototype` independently derives its complete additional storage
and structural-visit bounds first. The prototype explicitly accepts the Flat
prompt commitment and existing i32 final-output profile; it checks the prompt
length, token range and commitment, rooted class/artifact/job equality, format
caps, version, tokenizer and scheme before replay.

The collector constructs a fresh ordinary runner, so inherited custom selector,
drill injection and fused settings cannot change the capture. The new
`TirClassRunnerV1::run_observed` observes every node during that **single replay**
and produces the existing legacy leaves/roots from the same execution. It does
not call a cone evaluator a second time. `StepSink::try_node` has a default
implementation preserving existing sinks; a fallible observer stops at the
failing node. `TirExecutor::visit_state` borrows native state after a successful
step without exporting full `i128` state copies. A failure returns no completed
prototype; partial callbacks belong to an unsuccessful run and must be discarded.

Each immutable record has declared dtype, resolved row-major shape and native
typed storage (`Buf`). Physical executor views may be narrower or wider than
the declaration. Conversion checks the full mathematical integer against the
declared type; i64/i128 intermediates are never truncated into an i32 record.

| Record kind | Required identity and time |
|---|---|
| Node | absolute position, schedule occurrence, slot, block, layer, node, original commit flag |
| HistoryAppendRow | absolute source position, state and layer; final row of that position's `HistAppend` output before eviction |
| FixedAfter | position, state and layer; value after a successful whole step |
| LastOutput | every consumed post position, original declared shape/type, selected token |

Order is position, then canonical occurrence/node order (an appended history row
immediately follows its node), all Fixed instances in plan order, then consumed
output. A repeated block is distinguished by occurrence and slot, not only by
block/layer. Histories with `window=1` retain their appended source row even
though the executor's next-position window is empty. Full history windows are
already present where a node actually produces them; state callbacks do not add
another O(window × positions) payload copy.

## Proposed A1 source mapping — no new protocol assignment

The following is a data-access mapping for A1's independently derived
`ExpandedPlan`, not a successful checker result or accepted peer schedule:

| Independently derived relation | Local typed source |
|---|---|
| RecomputeNode | Node at `(ctx.pos, ctx.occurrence, node)`; verify slot/block/layer/dtype/resolved shape and take the checked lane range |
| Legacy committed node | Same Node and existing leaf's exact lane range |
| Legacy Fixed checkpoint | FixedAfter at the checkpoint's position/state/layer and lane range |
| Legacy history tile | HistoryAppendRow for each actual source position, oldest first, then the exact row lane subrange |
| Terminal selection/output | All consumed post Node/LastOutput records plus unchanged generated IDs, using the original profile |

A1 must rederive each relation from the plan index and bind its own checked
legacy frontier. A peer relation DTO or Panel record cannot nominate coverage.
Any future lookup index, source factory, parameter authentication, work/scratch
quote and fullrelation checker remain A1/A0 responsibilities; no per-lane full
checker or remote authenticated typed source is installed here.

The extra Panel commitment is represented only by the distinct opaque
`PendingTypedRecordCommitment` requirement. **No digest, hash domain, public ID,
wire decoder, signature, new scheme or native carrier is assigned.** It has no
equality, serialization or conversion to `Digest`, PC2R1REC, the old execution
root or credit. `validate_local_copy` compares every local field and value
against the original immutable artifact, in full; this is a local identity
comparison, not remote commitment authentication or arithmetic verification.
Candidate-copy memory belongs to its caller. A0 must assign and authenticate a
separate Panel-only commitment before this data can become a transported source.

## Bounds and cost model

All arithmetic is checked. Position/visit ceilings precede position loops.
Records, native lanes and native payload are counted from the original geometry,
including each step's Fixed values and every appended row. The existing
ExpandedPlan target-lane summary alone is insufficient for these additional
records. Empty occurrences, two geometry passes, observer node visits, two
state scans and retained-history visitor rows are reserved conservatively.
Actual vector capacities are checked against the derived backing quote.

The additional heap model includes both coexisting metadata/shape owners,
declared-width payload, bounded collector scratch, conservative format-ceiling
program/layout/context/Flat-prompt authentication buffers and fresh runner tile
metadata. Authentication scratch is capped before encoding/hashing. This is a
logical backing bound, not measured allocator/RSS or a complete process budget.
The already admitted program/params, executor live state, ordinary run's leaf
hashes/logits/generated output, arithmetic workspace and caller copies require
their own reservations. No native resource lease is granted by this quote.

Local storage is a fixed **cost model**, not an assigned file codec:
`192 + 4*decode + Σ(64 + 8*rank + declared_width*lanes)` bytes. No spool is
written or reopened in this increment; filesystem allocation, buffering,
fsync/recovery and partial-spool attacks remain untested until a storage codec
is assigned. `max_local_storage_bytes` refuses the model's over-budget jobs.

## Adopted wide final-output boundary

The separately adopted RFC-0043 / ADR-0211 profile uses signed i64 Q24 logits,
its own scheme, version-2 eight-byte step leaves, `PALWTRC2`, i128 Q48 sampling
keys and private court version 2. It preserves the old i32 profile. The source
documents were read-only references from the original checkout; its uncommitted
wide implementation was not copied or made an implicit prerequisite.

This checkpoint does not contain that output adapter. Declared wide final
logits are explicitly refused before legacy selection/casts. This refusal does
not revisit the adopted profile; integrating its existing explicit API is a
separate change. Uncommitted i64/i128 intermediates of the current profile are
captured exactly. No clipping, re-scaling, recommit or context shortening occurs.

## Verification boundary

The original sparse small fixture uses prefill 2/decode 3, repeated layer
occurrences, global/per-layer Fixed and Hist instances, windows 1 and 3, and
uncommitted values scaled by 2^40 and 2^80. Every node is compared with the
separate reference evaluator, state/output with reference execution, and the
ordinary and observed runs with the same original leaves and roots. Attacks
cover omitted/reordered/substituted coordinates or types, wrong output/origin,
all capture caps, bad context/prompt, early callback failure, late state rollback
and inherited selector/drill settings.

These are CPU small-fixture source tests. They do not establish real-LLM
performance, a completed extra commitment, fullrelation acceptance, native body
176 admission, protected disk installation, fees/locks, Gate C or activation.
Exact candidate results and raw evidence are attached separately after testing.
