# A3 — RFC-0043 explicit wide Q24 run, first increment

Base: exact A0 `5d14e927c`. Source dependencies are A1 `41a00f590`,
`b048ce4e1`, `9a3e952e9` (local aliases `76c8b1ac5`, `fab0bd862`,
`3ff1a356d`). They provide immutable V2 geometry, canonical mixed-width
preimages, the explicitly V2 binding validator and the payload-only byte bound.
Their historical test counts are not results for this increment.

## API and supported subset

`node::run_wide_q24_v2` adds `TirClassRunnerWideQ24V2`,
`TirJobRunWideQ24V2`, `TirLeafOutWideQ24V2`, `TirWideRunLimitsV2` and
`TirWideRunQuoteV2`. The runner borrows an actual `PalwTirStepSpaceV2`, matching
compiled plan and original parameters, plus the independently resolved class ID.
`run` and `run_observed` receive the original immutable job, prompt, independent
expected execution hash, explicit resource ceilings and a borrowed leaf callback.

This first API supports **fixed-length, Flat prompt IDs, greedy, ordinary CPU**
execution only. It checks prompt count and every token against the original
program token bound; verifies the Flat commitment and the exact original full
context hash; and uses the adopted wide scheme without changing tokenizer,
seed, maximum context, units or values. Non-Flat prompt forms are refused. It
provides no custom-selector, boundary-lie, fused/device, stop/prefix, FP or V4
adapter. The raw v2 context does not identify all such modes; mode authorization
must stay at the independently resolved calling entrypoint, not be guessed from
zero tokenizer/CU fields or the context version. This producer API does not
supply native admission for any mode.

The legacy `TirJobRunV1` and all of its Vec<Vec<i32>> fields, methods and
commitment bytes remain unchanged. Its constructor additionally refuses a wide
program even if a caller mutates the public legacy-space DTO. The existing
backend still constructs only the rejecting V1 step space. V1 CPU/process
cell-device creation, IPC Open and node cell verifiers now reject a wide program
before execution or parameter serialization; their old wire bytes are unchanged.
A raw manually wrapped V1 CPU stepper also refuses stepping; its old infallible
i32 getter explicitly asserts on a wide plan instead of truncating. No Hello2
implementation, public/default routing, network arming or registry eligibility
is added.

## One execution and canonical leaves

Both routes use the same `TirExecutor` kernels and the same fallible borrowed
`ObservedSink`/`TirRunObserverV1`. The wide host orchestration has one fresh
executor and one position loop. It never calls the V1 runner, recomputes the
model, materializes a legacy run or reads the dirty checkout as source. The old
host driver remains intact apart from sharing the observer constructor.

Each newly produced node/Fixed/Hist tile is rederived at the next original
canonical coordinate, then passed to A1's `palw_tir_leaf_preimage_v2` using the
actual job. That API alone selects leaf version 2/eight-byte signed lanes for
the actual final post i64 logits, and the original version 1/four-byte bytes for
all other committed nodes, Fixed checkpoints and history. The unchanged
validator limits states to I8/I16/I32; Idx states are refused. High-bit Idx node
lanes use their original unsigned native values. All new wire coordinate
conversions are checked.

The canonical preimage helper is used rather than the aggregate leg builder:
the latter currently consumes/discards its private saved-hash vector at finish.
The runner keeps one exact-capacity saved-hash vector, hashes each emitted
preimage once, checks each position's cumulative geometry and the final count,
then uses the existing step Merkle function. A1's canonical aggregate builder
and independent manual hash/tree logic are separate test oracles. Commitments
bind data identity and original context; they do not certify arithmetic.

Native final values are checked and copied directly to Vec<Vec<i64>>, with no
intermediate i32 row. Greedy selection uses the adopted i128 Q48 key primitive;
ties retain the lowest index. Trace/execution/output roots use the original
context and unchanged existing domain APIs. Observer failure returns no run;
all partial observations and leaf deliveries must be discarded. No later model
position is run after a failed observer callback.

## Finite request and ownership

`quote` is closed-form: it does not enumerate positions/leaves or encode a full
program. It checks positions, leaves, row lanes, symbolic work and simultaneous
requested bytes before execution. Checked symbolic node costs cover every
scheduled occurrence at its maximum legal history extent, including post at
every position conservatively. Host terms also charge canonical geometry,
metadata/hashing, selection, prompt scans, repeated empty blocks and the
Fixed/Hist visitor scans. These are logical cost units, not physical instruction
or timing measurements and not native court prices.

The shared executor-memory calculation is factored over borrowed plan,
positions and h_tile. V1's numerical formula is preserved; program byte length
uses allocation-free Borsh counting. Wide retained eight-byte rows, row headers,
IDs and saved hashes are added separately, together with simultaneous step/row/
tile Merkle scratch. Tile scratch explicitly includes the co-live i128 values,
encoded preimage and growing hash writer (five times the eight-byte payload
bound plus header slack). The payload-only A1 bound does not stand for the
26-byte preimage header, DTO, opening, hashes or tree workspace. Prompt-writer
scratch is separate.

The optional existing host memory callback is invoked before executor/vector
allocation and hashing. Its guard stays in the immutable, non-Clone returned
run. Keeping the entire conservative peak reservation until result Drop is
intentional over-reservation. In absence of a callback the same explicit logical
ceilings still apply; this is not a measured RSS or global allocator guarantee.
Caller-owned plan/params/space/prompt and callback work/retained copies require
separate owner reservations. Retrying an execution is a new producer request,
not a refund of any Court service budget.

## Validation and follow-up

New local paired declarations exercise i64 MIN/MAX, all-negative rows, the
1082*2^24 range, tie-breaking, original tokenizer0 versus class metadata18,
window1/3, narrow checkpoints/history, checked bounds and observer aborts.
An additional independent paired fixture checks high-bit Idx nodes and signed
I32 state words, and rejects Idx states structurally. A separate-binary host
callback test records logical reservation bytes, verifies an active lease at
callbacks and after return, then zero after Drop, binding refusal and host
refusal. This measures guard lifecycle only, not physical memory.
Neither declaration has a native registration/admission receipt. These are
local executor/commitment checks, not a trained LLM reproduction or native credit.

Separate later increments must connect bounded `PALWTRC2` capture/authentication,
Hello2/cell stepping, evidence/Court and SDK/native callers. Old `PALWTIRC`,
Hello1/IPC V1 and Vec<i32> consumers must not accept wide rows by casting or by
reinterpreting their old bytes. Real trained-cohort reproduction at its original
maximum context remains a later SDK/native-wide step. No toy result is counted
as that outcome. Public activation remains unchanged.
