# A3 — PALWTRC2 bounded data capture, separate increment

Base: A3 run source `79b8804d9` and evidence `ee6299930`, preserved on
`codex/a3-wide-run-v2`. New branch: `codex/a3-wide-capture-v2`. This is the
next RFC-0043 source increment. Its source review and execution evidence must
remain separate from the previous 49-test run result. At initial source freeze
capture2 has **not been compiled/tested**; A0 requested source/review first while
Root's current run and disk-space check finish. Hello2/IPC is a later commit.

## Version and API contract

`node::wide_capture_v2::TirWideCaptureV2` has magic **PALWTRC2**. The Borsh body is
exactly: existing BindingV1 DTO (binding version1), prompt Vec<u32>,
logits_rows Vec<Vec<i64>>, generated Vec<u32>, leaves Vec<generic old leaf DTO>.
Final post i64 leaves have version2/eight-byte lanes; other committed nodes,
Fixed checkpoints and Hist remain version1/four-byte lanes, under A1's strict
V2 geometry. State dtype rules remain I8/I16/I32; Idx states stay rejected.
No new native tag/domain/record ID is invented. The adopted magic identifies
this separate data format, not a public routing or activation decision.

`TirCaptureV1`, PALWTIRC, i32 rows, old IPC and old consumer dispatch retain their
meaning and wire bytes. Readers reject the other magic. There is no unbounded
BorshDeserialize implementation for the new capture. Public fields, mutation,
Clone and default successful limits are not provided; immutable getters expose
the retained data and explicit copies are caller-owned.

The shared capture Reader parses only fixed-size scalar types through generic
Borsh. Every Vec is separately framed and charged before reserve. V2 uses wire
width8 for i64 lanes; outer row lengths still need only four wire bytes per
header, and leaf headers still need26. It checks metadata format/count bounds,
frame length, native decoded bytes, prompt/row/lane/leaf counts, each leaf payload
and total payload, and strict EOF. V1 uses its exact old 2*frame+16MiB decoded
budget and unchanged per-field acceptance/error behavior.

`from_run(expected_binding, prompt, &WideRun, owned_leaves, limits)` copies an
already completed immutable run's rows, IDs and original roots, without invoking
any inference/replay API. The supplied leaf Vec is already acquired/reserved by
the caller; ownership is moved, including all spare capacity. Only zero or the
full original leaf count is accepted. Those leaves remain unauthenticated until
explicit authentication. New binding/prompt/rows/IDs copies use exact requested
capacities. Original canonical job fields (including tokenizer0 separately from
registered metadata18), program, class, artifact, values, units and maximum
context are not rewritten.

`decode_bounded`, `quote_from_run`, `quote_encode`, `encode_bounded`,
`quote_auth_expected` and `authenticate_expected` take explicit limits. Limits
and quotes describe each operation's logical backing; they are **not actual host
leases** or physical RSS guarantees. Encoded output is one exact-sized Vec,
serialized directly after counting, with no MAGIC Vec growth or duplicate body.
Caller retention of previous outputs/copies requires an external reservation.

## Simultaneous ownership inventory

| Operation | Counted simultaneous owners |
|---|---|
| Decode | supplied input-frame span + new capture header/all requested native Vec backing + fixed8KiB parser/assembly scratch + caller external reservation |
| Build from run | original run's conservative reserved_bytes + new capture private copies + moved leaf Vec actual capacity/payload capacities once + identity/copy scratch + caller external reservation |
| Encode | actual retained capture capacities + one new encoded frame + fixed8KiB serializer scratch + caller external reservation |
| Authenticate | actual retained capture capacities + metadata/prompt hash scratch + dense saved hashes/Merkle, row/tile Merkle scratch, temporary i128 lane reads/hash writer + caller external reservation |

`moved_leaf_bytes` is an informational subset of `capture_bytes`; it is never
summed twice. Native backing includes every outer row/leaf header and every
inner buffer's capacity. Build checks total backing before private copies or
identity hashing; decode checks each new reserve against both decoded and joint
operation ceilings; encode counts/checks before allocating the output frame.

For an operation where the original run, input frame, prior encoded outputs,
expected metadata, pre-admitted verified space, parameters, plan or other
independent buffers are still retained but not already counted in that row,
**the caller must pre-sum them into external_retained_bytes**. Unused capacity
outside a borrowed input-frame slice is also caller-owned. These declarations
are not verified physical allocations. Authentication at least checks that its
external declaration covers the known independently owned expected binding.
The opaque verified space's admission remains its owning caller's responsibility.
Independent operations do not constitute a persistent Court service ledger;
retry/request budgets and real native host leases belong to the future caller.
No successful data operation authorizes new credit or refunds spent Court work.

## Independent expected data, not computation truth

Authentication requires both the independently rooted expected BindingV1 and an
independently constructed/pre-admitted opaque PalwTirVerifiedBindingV2. The token
alone does not retain expected roots, so literal equality of all delivered and
expected class/job/artifact/roots is still required. Auth also pins the token's
class ID/context/job, exact space layout and program bytes to that expected
binding. Program bytes are compared with a streaming Borsh writer; it never
calls generic program decoding or constructs a new StepSpace under capture
scratch limits. The caller must admit that earlier construction separately.

It derives expected count/vocabulary/geometry from the borrowed immutable V2
space, checks original prompt and row/ID shapes, recomputes the wide data trace,
and, for a dense capture, validates each next canonical coordinate/version/width,
then original hash and step root. A1's disclosed-value reader checks geometry
without rejecting a narrow out-of-range value as a different kind of delivery.
Sparse means exactly zero leaves; any nonzero partial count is refused.

This is **data identity only**. A coherently rooted bad ID, a non-greedy chosen
ID, or disagreement between separately committed row and step values remains
available as evidence for a deterministic checker. This API does not run a
selector/model, infer arithmetic truth, convict a Miner or set any Court outcome.
No signature or rooted expected binding is invented by this library. A caller
must not derive its expected roots from the delivered candidate and call that
independent authentication.

## Review and validation boundary

The new independent test source uses a shadow five-field Borsh DTO, exact native
i64 endpoints, both dense and fold-only captures from one existing run, hostile
lengths/header amplification/truncation, independent expected-binding/space
substitutions, coherently rehashed malformed dense geometry, wrong selection/
OOV evidence and complete per-operation ownership-boundary cases. These are
planned same-source local checks until a recorded execution succeeds.

Existing capture codec and wide-run regression targets will run only after
source freeze, independent review and A0's build/disk release. No actual trained
LLM reproduction, native registration/admission, real host lease/RSS benchmark,
FP/V4/device support, Gate C, funding/fees or public activation is claimed. The
next Hello2/cell implementation remains separate; V1 refusal guards stay active.
